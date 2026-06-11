//! One-off: read a single movetext line on stdin, print the UCI move list,
//! plus the FEN before the final move (for explorer lookups).
use shakmaty::{fen::Fen, san::San, uci::UciMove, CastlingMode, Chess, EnPassantMode, Position};

fn main() {
    let mut line = String::new();
    std::io::stdin().read_line(&mut line).unwrap();
    let mut pos = Chess::default();
    let mut ucis: Vec<String> = Vec::new();
    let mut pre_final = Fen::from_position(pos.clone(), EnPassantMode::Legal).to_string();
    for tok in line.split_whitespace() {
        if tok.ends_with('.') || tok == "1-0" || tok == "0-1" || tok == "1/2-1/2" || tok == "*" {
            continue;
        }
        let san: San = tok.trim_end_matches(['!', '?']).parse().unwrap();
        let mv = san.to_move(&pos).unwrap();
        pre_final = Fen::from_position(pos.clone(), EnPassantMode::Legal).to_string();
        ucis.push(UciMove::from_move(&mv, CastlingMode::Standard).to_string());
        pos = pos.play(&mv).unwrap();
    }
    println!("plies: {}", ucis.len());
    println!("play: {}", ucis.join(","));
    println!("pre_final_fen: {pre_final}");
    println!("last_uci: {}", ucis.last().unwrap());
}
