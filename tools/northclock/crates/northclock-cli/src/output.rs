use northclock_core::CommandEnvelope;

pub(crate) fn emit(envelope: &CommandEnvelope, json: bool) {
    if json {
        match serde_json::to_string(envelope) {
            Ok(value) => println!("{value}"),
            Err(error) => eprintln!("could not serialize command result: {error}"),
        }
        return;
    }
    println!("{}: {:?}", envelope.command, envelope.status);
    if let Some(capability) = &envelope.capability {
        println!(
            "capability={} state={:?} backend={} hardware_verified={}",
            capability.name, capability.state, capability.backend, capability.hardware_verified
        );
        println!("{}", capability.detail);
    }
    if let Some(data) = &envelope.data {
        match serde_json::to_string_pretty(data) {
            Ok(value) => println!("{value}"),
            Err(error) => eprintln!("could not serialize command data: {error}"),
        }
    }
    if let Some(error) = &envelope.error {
        eprintln!("{}", error.message);
    }
}
