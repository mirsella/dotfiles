use super::*;
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::TcpListener,
};

#[test]
fn convergence_preserves_existing_password() -> Result<()> {
    let record = json!({"id":"u1","email":"a@b","role":"user"});
    let desired =
        json!({"email":"a@b","role":"admin","password":"secret","passwordConfirm":"secret"});
    let patch = record_patch(&record, &desired, &["password", "passwordConfirm"])?;
    assert_eq!(patch.len(), 1);
    assert_eq!(patch["role"], "admin");
    Ok(())
}

#[test]
fn api_encodes_filters_sends_json_and_preserves_auth_errors() -> Result<()> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let api = BeszelApi {
        hub: format!("http://{}", listener.local_addr()?),
        agent: util::agent(5),
        token: "test-token".into(),
    };
    let server = thread::spawn(move || -> Result<()> {
        for status in [200, 401] {
            let (mut stream, _) = listener.accept()?;
            stream.set_read_timeout(Some(Duration::from_secs(5)))?;
            let mut reader = BufReader::new(stream.try_clone()?);
            let mut start = String::new();
            reader.read_line(&mut start)?;
            let mut headers = std::collections::HashMap::new();
            loop {
                let mut line = String::new();
                ensure!(reader.read_line(&mut line)? > 0, "truncated request");
                if line == "\r\n" {
                    break;
                }
                let (key, value) = line.split_once(':').context("invalid header")?;
                headers.insert(key.to_ascii_lowercase(), value.trim().to_owned());
            }
            assert_eq!(headers["authorization"], "test-token");
            if status == 200 {
                let parts: Vec<_> = start.split_whitespace().collect();
                assert_eq!(parts.len(), 3);
                assert_eq!(parts[0], "GET");
                assert!(parts[1].starts_with("/records?filter="));
                assert!(parts[1].contains("%26") && parts[1].contains("%C3%BC"));
                assert!(!parts[1].contains('&'));
            } else {
                assert!(start.starts_with("POST /auth "));
                assert_eq!(headers["content-type"], "application/json");
                let mut body = vec![0; headers["content-length"].parse()?];
                reader.read_exact(&mut body)?;
                assert_eq!(
                    serde_json::from_slice::<Value>(&body)?,
                    json!({"identity":"user"})
                );
            }
            write!(
                stream,
                "HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{{}}"
            )?;
        }
        Ok(())
    });
    api.get("/records", &[("filter", "name = \"pi & ü\"")])?;
    let error = api
        .write(Method::POST, "/auth", json!({"identity":"user"}))
        .unwrap_err();
    assert!(matches!(
        error.downcast_ref::<ureq::Error>(),
        Some(ureq::Error::StatusCode(401))
    ));
    assert!(error.to_string().contains("POST /auth"));
    server.join().expect("HTTP fixture panicked")?;
    Ok(())
}
