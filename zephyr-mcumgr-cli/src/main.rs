fn main() {
    println!("Hello world!");

    for port in serialport::available_ports().unwrap() {
        println!("{:#?}", port);
    }
}
