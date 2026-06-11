//! One-off: print position count and total games (root move-count sum).
use shakmaty::Chess;

fn main() {
    let path = std::env::args().nth(1).expect("usage: bookstats <book>");
    let bytes = std::fs::read(&path).unwrap();
    let book = shared::book::Book::open(&bytes[..]).unwrap();
    let root = shared::position_hash(&Chess::default());
    let stats = book.lookup(root);
    let games: u64 = stats.iter().map(|m| m.count).sum();
    println!("positions: {}", book.position_count());
    println!("root moves: {}", stats.len());
    println!("games: {}", games);
    println!("file bytes: {}", bytes.len());
}
