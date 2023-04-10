CARGO_INCREMENTAL=0 RUSTFLAGS='-Cinstrument-coverage' LLVM_PROFILE_FILE='coverage-%p-%m.profraw' cargo test
mkdir -p target/coverage
grcov . --binary-path ./target/debug/deps/ -s ./src/ -t lcov --branch --ignore-not-existing --ignore '../*' --ignore "/*" --excl-line "///" -o target/coverage/tests.lcov
