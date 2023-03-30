// Copyright 2023, Offchain Labs, Inc.
// For license information, see https://github.com/OffchainLabs/nitro/blob/master/LICENSE

#![no_main]

use arbitrum::{contract, debug, Bytes20, Bytes32};

arbitrum::arbitrum_main!(user_main);

fn user_main(input: Vec<u8>) -> Result<Vec<u8>, Vec<u8>> {
    let mut input = input.as_slice();
    let count = input[0];
    input = &input[1..];

    // combined output of all calls
    let mut output = vec![];

    debug::println(format!("Calling {count} contract(s)"));
    for _ in 0..count {
        let length = u32::from_be_bytes(input[..4].try_into().unwrap()) as usize;
        input = &input[4..];

        let next = &input[length..];
        let mut curr = &input[..length];

        let kind = curr[0];
        curr = &curr[1..];

        let mut value = None;
        if kind == 1 {
            value = Some(Bytes32::from_slice(&curr[..32]).unwrap());
            curr = &curr[32..];
        }

        let addr = Bytes20::from_slice(&curr[..20]).unwrap();
        let data = &curr[20..];
        debug::println(match value {
            Some(value) => format!(
                "Calling {addr} with {} bytes and value {} {kind}",
                hex::encode(&value),
                data.len()
            ),
            None => format!("Calling {addr} with {} bytes {kind}", curr.len()),
        });

        let return_data = match kind {
            0 => contract::call(addr, data, value, None)?,
            1 => contract::call(addr, data, value, None)?, // nonzero value
            2 => contract::delegate_call(addr, data, None)?,
            3 => contract::static_call(addr, data, None)?,
            x => panic!("unknown call kind {x}"),
        };
        if !return_data.is_empty() {
            debug::println(format!(
                "Contract {addr} returned {} bytes",
                return_data.len()
            ));
        }
        output.extend(return_data);
        input = next;
    }

    Ok(output)
}