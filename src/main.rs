use umor::tokenize;

fn main() {
    let src = "「こんにちは。」を　表示する";
    match tokenize(src) {
        Ok(tokens) => {
            for token in tokens {
                println!("{:?}", token);
            }
        }
        Err(e) => eprintln!("{e}"),
    }
}
