use shot::Capturer;

fn main() {
    let mut capturer = Capturer::new().unwrap();
    capturer.capture_output().unwrap();
}
