use cold_clear_2::data::{Board, Piece};
use cold_clear_2::movegen::find_moves;
use criterion::{criterion_group, criterion_main, Criterion};

const PIECES: [Piece; 7] = [
    Piece::I,
    Piece::O,
    Piece::T,
    Piece::L,
    Piece::J,
    Piece::S,
    Piece::Z,
];

fn bench_movegen(c: &mut Criterion, name: &str, board: Board) {
    let mut group = c.benchmark_group(name);
    for p in PIECES {
        group.bench_function(format!("{:?}", p), |b| b.iter(|| find_moves(&board, p)));
    }
}

fn bench(c: &mut Criterion) {
    bench_movegen(c, "empty", Board::default());

    // v115@egA8IeC8FeE8DeF8CeH8BeH8CeH8AeD8JeAgH
    #[rustfmt::skip]
    bench_movegen(c, "tspin", Board {
        cols: [
            0b00111111,
            0b00111111,
            0b00011111,
            0b00000111,
            0b00000001,
            0b00000000,
            0b00001101,
            0b00011111,
            0b00111111,
            0b11111111,
        ]
    });

    // v115@LgB8HeD8BeH8CeI8AeH8BeH8CeH8AeI8AeH8AeD8Je?AgH
    #[rustfmt::skip]
    bench_movegen(c, "dtd", Board {
        cols: [
            0b111111111,
            0b111111111,
            0b011111111,
            0b011111111,
            0b000111111,
            0b000100110,
            0b010000001,
            0b011110111,
            0b011111111,
            0b011111111,
        ]
    });

    // v115@vfH8BeH8IeA8IeH8BeH8BeB8HeB8HeB8BeH8BeH8Ie?A8SeAgH
    #[rustfmt::skip]
    bench_movegen(c, "terrible", Board {
        cols: [
            0b000011111111,
            0b000011000000,
            0b110011000000,
            0b110011001100,
            0b110011001100,
            0b110011001100,
            0b110011001100,
            0b110000001100,
            0b110000001100,
            0b111111111100,
        ]
    });
}

criterion_group!(benchmark, bench);

criterion_main!(benchmark);
