#![no_main]

use libfuzzer_sys::fuzz_target;
use northclock_driver_protocol::CurveOptimizerRequest;

fuzz_target!(|bytes: &[u8]| {
    if let Ok(request) = CurveOptimizerRequest::decode(bytes) {
        let _ = request.validate();
        let _ = request.encode();
    }
});
