//! Generate test vectors for TypeScript interop testing.
//!
//! Run with: cargo run --package protocol --example test_vectors

use protocol::messages::*;

fn main() {
    // Test vector 1: Simple Ping
    let ping = Envelope::new(
        1,
        Message::Ping(Ping {
            timestamp: 12345,
            payload: vec![],
        }),
    );
    print_test_vector("ping", &ping);

    // Test vector 2: SessionCreate with defaults
    let session_create = Envelope::new(
        2,
        Message::SessionCreate(SessionCreate {
            cols: 80,
            rows: 24,
            shell: None,
            env: vec![],
            cwd: None,
        }),
    );
    print_test_vector("session_create_default", &session_create);

    // Test vector 3: SessionData with Stdout
    let session_data = Envelope::new(
        3,
        Message::SessionData(SessionData {
            session_id: "sess-1".to_string(),
            stream: DataStream::Stdout,
            data: b"Hello".to_vec(),
        }),
    );
    print_test_vector("session_data", &session_data);

    // Test vector 4: Error message
    let error = Envelope::new(
        4,
        Message::Error(ErrorMessage {
            code: ErrorCode::NotFound,
            message: "Not found".to_string(),
            context: Some("test".to_string()),
            recoverable: false,
        }),
    );
    print_test_vector("error", &error);

    // Test vector 5: FileListResponse
    let file_list = Envelope::new(
        5,
        Message::FileListResponse(FileListResponse {
            path: "/home".to_string(),
            entries: vec![FileEntry {
                name: "test.txt".to_string(),
                entry_type: FileEntryType::File,
                size: 100,
                mode: 0o644,
                modified: 1704067200,
            }],
        }),
    );
    print_test_vector("file_list_response", &file_list);

    // Test vector 6: Capabilities
    let capabilities = Envelope::new(6, Message::Capabilities(Capabilities::default()));
    print_test_vector("capabilities", &capabilities);
}

fn print_test_vector(name: &str, envelope: &Envelope) {
    let bytes = envelope.to_msgpack().expect("serialization failed");
    print!("export const {} = new Uint8Array([", name);
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 {
            print!(", ");
        }
        print!("{}", b);
    }
    println!("]);");
}
