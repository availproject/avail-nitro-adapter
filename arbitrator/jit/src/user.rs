// Copyright 2022-2023, Offchain Labs, Inc.
// For license information, see https://github.com/nitro/blob/master/LICENSE

use crate::{gostack::GoStack, machine::WasmEnvMut};
use arbutil::heapify;
use eyre::eyre;
use prover::programs::prelude::*;
use std::mem;
use stylus::{
    native::{self, NativeInstance},
    run::RunProgram,
};

/// Compiles and instruments user wasm.
/// go side: λ(wasm []byte, version u32) (machine *Machine, err *Vec<u8>)
pub fn compile_user_wasm(env: WasmEnvMut, sp: u32) {
    let mut sp = GoStack::simple(sp, &env);
    let wasm = sp.read_go_slice_owned();
    let config = StylusConfig::version(sp.read_u32());
    sp.skip_space();

    match native::module(&wasm, config) {
        Ok(module) => {
            sp.write_ptr(heapify(module));
            sp.write_nullptr();
        }
        Err(error) => {
            let error = format!("failed to compile: {error:?}").as_bytes().to_vec();
            sp.write_nullptr();
            sp.write_ptr(heapify(error));
        }
    }
}

/// Links and executes a user wasm.
/// go side: λ(mach *Machine, data []byte, params *StylusConfig, gas *u64, root *[32]byte) (status byte, out *Vec<u8>)
pub fn call_user_wasm(env: WasmEnvMut, sp: u32) {
    let mut sp = GoStack::simple(sp, &env);
    let module: Vec<u8> = unsafe { *Box::from_raw(sp.read_ptr_mut()) };
    let calldata = sp.read_go_slice_owned();
    let config: StylusConfig = unsafe { *Box::from_raw(sp.read_ptr_mut()) };

    // buy ink
    let pricing = config.pricing;
    let gas = sp.read_go_ptr();
    let ink = pricing.gas_to_ink(sp.read_u64_raw(gas));

    // skip the root since we don't use these
    sp.skip_u64();

    // Safety: module came from compile_user_wasm
    let instance = unsafe { NativeInstance::deserialize(&module, config.clone()) };
    let mut instance = match instance {
        Ok(instance) => instance,
        Err(error) => panic!("failed to instantiate program {error:?}"),
    };
    instance.set_ink(ink);
    instance.set_stack(config.depth.max_depth);

    let status = match instance.run_main(&calldata, &config) {
        Err(err) | Ok(UserOutcome::Failure(err)) => {
            let outs = format!("{:?}", err.wrap_err(eyre!("failed to execute program")));
            sp.write_u8(UserOutcomeKind::Failure as u8).skip_space();
            sp.write_ptr(heapify(outs.into_bytes()));
            UserOutcomeKind::Failure
        }
        Ok(outcome) => {
            let (status, outs) = outcome.into_data();
            sp.write_u8(status as u8).skip_space();
            sp.write_ptr(heapify(outs));
            status
        }
    };
    let ink_left = match status {
        UserOutcomeKind::OutOfStack => 0, // take all gas when out of stack
        _ => instance.ink_left().into(),
    };
    sp.write_u64_raw(gas, pricing.ink_to_gas(ink_left));
}

/// Reads the length of a rust `Vec`
/// go side: λ(vec *Vec<u8>) (len u32)
pub fn read_rust_vec_len(env: WasmEnvMut, sp: u32) {
    let mut sp = GoStack::simple(sp, &env);
    let vec: &Vec<u8> = unsafe { &*sp.read_ptr() };
    sp.write_u32(vec.len() as u32);
}

/// Copies the contents of a rust `Vec` into a go slice, dropping it in the process
/// go side: λ(vec *Vec<u8>, dest []byte)
pub fn rust_vec_into_slice(env: WasmEnvMut, sp: u32) {
    let mut sp = GoStack::simple(sp, &env);
    let vec: Vec<u8> = unsafe { *Box::from_raw(sp.read_ptr_mut()) };
    let ptr: *mut u8 = sp.read_ptr_mut();
    sp.write_slice(ptr as u64, &vec);
    mem::drop(vec)
}

/// Creates a `StylusConfig` from its component parts.
/// go side: λ(version, maxDepth u32, inkPrice, hostioInk u64) *StylusConfig
pub fn rust_config_impl(env: WasmEnvMut, sp: u32) {
    let mut sp = GoStack::simple(sp, &env);
    let version = sp.read_u32();

    let mut config = StylusConfig::version(version);
    config.depth.max_depth = sp.read_u32();
    config.pricing.ink_price = sp.read_u64();
    config.pricing.hostio_ink = sp.read_u64();
    sp.write_ptr(heapify(config));
}