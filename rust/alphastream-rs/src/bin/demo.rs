fn main() {
    let ver = libalphastream::version();
    let out = libalphastream::echo("hello alphastream");
    println!("alphastream-rs v{} — echo: {}", ver, out);
}
