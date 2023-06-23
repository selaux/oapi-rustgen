use oapi_rustgen::{Analyzer, ClientWriter, TypesWriter, ServerWriter};

const JSON: &str = include_str!("./petstore-expanded.json");

fn main() {
    let analysis = Analyzer::new().run(JSON).expect("success");
    let types_tokens = TypesWriter::new(&analysis)
        .write()
        .expect("generation should work")
        .to_string()
        .expect("should be convertible to string");
    let client_tokens = ClientWriter::new(&analysis)
        .write()
        .expect("generation should work")
        .to_string()
        .expect("should be convertible to string");
    let server_tokens = ServerWriter::new(&analysis)
        .write()
        .expect("generation should work")
        .to_string()
        .expect("should be convertible to string");

    let client = format!("{}\n\n{}", types_tokens, client_tokens);
    let client = rustfmt_wrapper::rustfmt(client)
        .expect("should format");
    let server = format!("{}\n\n{}", types_tokens, server_tokens);
    let server = rustfmt_wrapper::rustfmt(server)
        .expect("should format");

    std::fs::write("src/client_gen.rs", client).expect("should write to a file");
    std::fs::write("src/server_gen.rs", server).expect("should write to a file");
}
