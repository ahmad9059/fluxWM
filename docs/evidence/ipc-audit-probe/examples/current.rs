//! Checks corrected IPC invariants; historical main/transcript remain unchanged.
use flux_ipc::*;
use std::{
    io::{Read, Write},
    net::Shutdown,
    os::unix::{fs::DirBuilderExt, net::UnixStream},
    time::{Duration, Instant},
};
fn main() {
    let dir = std::env::temp_dir().join(format!("flux-ipc-audit-{}", std::process::id()));
    std::fs::DirBuilder::new().mode(0o700).create(&dir).unwrap();
    let mut server = Server::bind(&dir, "probe").unwrap();
    let mut client = UnixStream::connect(server.socket_path()).unwrap();
    client
        .write_all(&encode(&Request::new("half-close", Method::Version)).unwrap())
        .unwrap();
    client.shutdown(Shutdown::Write).unwrap();
    let mut dispatched = 0;
    server.poll(Instant::now(), |r| {
        dispatched += 1;
        Response::success(r.id, 0, serde_json::Value::Null)
    });
    server.poll(Instant::now(), |_| unreachable!());
    println!(
        "half_close: dispatched={dispatched} remaining_connections={}",
        server.connection_count()
    );
    assert_eq!(dispatched, 1);
    assert_eq!(server.connection_count(), 0);
    drop(client);
    let mut client = UnixStream::connect(server.socket_path()).unwrap();
    client
        .set_read_timeout(Some(Duration::from_secs(1)))
        .unwrap();
    let valid = Request::new("mutation", Method::ConfigReload);
    let mut invalid = Request::new("invalid-after-mutation", Method::Version);
    invalid.version = 2;
    let mut frames = encode(&valid).unwrap();
    frames.extend(encode(&invalid).unwrap());
    client.write_all(&frames).unwrap();
    server.set_state_sequence(10);
    server.poll(Instant::now(), |r| {
        Response::success(r.id, 11, serde_json::Value::Null)
    });
    let mut sequences = Vec::new();
    for _ in 0..2 {
        let mut prefix = [0; 4];
        client.read_exact(&mut prefix).unwrap();
        let mut payload = vec![0; u32::from_be_bytes(prefix) as usize];
        client.read_exact(&mut payload).unwrap();
        let response: Response = decode(&payload).unwrap();
        println!(
            "batch_sequence: id={} sequence={}",
            response.id, response.state_sequence.0
        );
        sequences.push(response.state_sequence.0);
    }
    assert_eq!(sequences, vec![11, 11]);
    let mut deep = serde_json::Value::Null;
    for _ in 0..MAX_DEPTH + 1 {
        deep = serde_json::json!([deep]);
    }
    let encoded = encode(&Response::success("deep", 0, deep));
    println!("response_depth: encode_ok={}", encoded.is_ok());
    assert!(encoded.is_err());
    let mut empty_path = Request::new("spaces", Method::ConfigCheck);
    empty_path.params = serde_json::json!({"path":""});
    println!(
        "empty_config_path: accepted={}",
        empty_path.validate().is_ok()
    );
    assert!(empty_path.validate().is_err());
    for name in ["quit", "lock", "reload-config", "close-window"] {
        let mut action = Request::new("extra-unit-field", Method::Action);
        action.params = serde_json::json!({"name": name, "unexpected": true});
        let accepted = action.validate().is_ok();
        println!("unit_action_unknown_field: name={name} accepted={accepted}");
        assert!(!accepted);
    }
    drop(client);
    drop(server);
    std::fs::remove_dir_all(dir).unwrap();
}
