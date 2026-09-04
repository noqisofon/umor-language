use umor::tokenize;

fn main() {
    let src = "「こんにちは。」を　表示する";
    for token in tokenize(src) {
        println!("{:?}", token);
    }
}
