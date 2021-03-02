use rand::Rng;

// String::arbitrary is not so good sometimes as it doesn't generate printable strings
// not ideal as it doesn't generate '\n',' ','/' and other valid characters
// does some really arbitrary crap logic but should be fine
pub fn generate_sane_string(range: std::ops::Range<usize>) -> String {
    let mut newlines = rand::thread_rng().gen_range(0..10);
    let size = rand::thread_rng().gen_range(range);
    let mut s = String::new();

    loop {
        s.extend(
            rand::thread_rng()
                .sample_iter(&rand::distributions::Alphanumeric)
                .take(size / (newlines + 1))
                .map(char::from),
        );

        if newlines == 0 {
            break;
        }
        newlines -= 1;
        s.push('\n');
    }
    s
}

