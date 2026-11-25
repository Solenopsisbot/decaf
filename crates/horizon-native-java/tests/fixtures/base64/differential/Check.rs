// Compares `base64_codec.rs` with the JDK's output, line by line. See README.md.
//
//   rustc -O --edition 2024 Check.rs -o check
//   ./check decode decode.txt   (DecodeAll's output)
//   ./check encode encode.txt   (EncodeAll's output)
#[path = "../../../../src/base64_codec.rs"]
#[allow(dead_code)]
mod codec;
use codec::*;
use std::io::{BufRead, BufReader};

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len() / 2)
        .map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap())
        .collect()
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn main() {
    let mode = std::env::args().nth(1).unwrap();
    let path = std::env::args().nth(2).unwrap();
    let file = BufReader::new(std::fs::File::open(path).unwrap());
    let (mut checked, mut bad) = (0u64, 0u64);
    for line in file.lines() {
        let line = line.unwrap();
        let p: Vec<&str> = line.splitn(3, '|').collect();
        let wrong = if mode == "decode" {
            let kind = match p[1] {
                "0" => DecoderKind::Basic,
                "1" => DecoderKind::Url,
                _ => DecoderKind::Mime,
            };
            let got = match decode(kind, &unhex(p[0])) {
                Ok(bytes) => format!("OK:{}", hex(&bytes)),
                Err(message) => format!("E:{message}"),
            };
            (got != p[2]).then(|| format!("{line} got {got}"))
        } else {
            // url|padding|line length|separator|input|output|encodeToString's length
            let p: Vec<&str> = line.split('|').collect();
            let (line_max, separator) = if p[2] == "mmime" {
                (76, vec![13u8, 10])
            } else {
                let length: i32 = p[2][1..].parse().unwrap();
                (
                    mime_line_max(length),
                    if p[3] == "-" { vec![] } else { unhex(p[3]) },
                )
            };
            let config = EncoderConfig {
                url: p[0] == "u1",
                padding: p[1] == "p1",
                line_max,
                separator,
            };
            let input = unhex(p[4]);
            let got = encode(&config, &input);
            let length_ok = encoded_len(&config, input.len()) == got.len();
            (hex(&got) != p[5] || !length_ok).then(|| format!("{line} got {}", hex(&got)))
        };
        checked += 1;
        if let Some(wrong) = wrong {
            bad += 1;
            if bad < 20 {
                println!("MISMATCH {wrong}");
            }
        }
    }
    println!("checked {checked}, mismatches {bad}");
}
