use froggi::response::{Item, Response};

use std::io::Write;
use std::net::{TcpListener, TcpStream};

fn handle_client(mut stream: TcpStream) {
    let request = froggi::request::Request::from_bytes(&mut stream).unwrap();

    println!(
        "request (version {}, length {}): {}",
        request.version(),
        request.path().len(),
        request.path()
    );

    // todo: verify markup is correct
    // todo: some sort of page and page data cache
    let page = String::from(include_str!("../pages/index.fml"));
    let header_img_data = include_bytes!("../pages/header.jpg");
    let mut header_img = Vec::new();
    header_img.extend_from_slice(header_img_data);

    let response = Response::new(
        page,
        vec![Item::new(String::from("header.jpg"), header_img)],
    );

    stream.write_all(&response.into_bytes()).unwrap();
}

fn main() {
    froggi::hello();
    let listener = TcpListener::bind("0.0.0.0:11121").unwrap();
    println!("listening");
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                println!("new client");
                std::thread::spawn(move || handle_client(stream));
            }
            Err(e) => {
                println!("error {}", e);
            }
        }
    }
}
