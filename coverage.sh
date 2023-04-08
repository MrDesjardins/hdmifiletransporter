CARGO_INCREMENTAL=0 RUSTFLAGS='-Cinstrument-coverage' LLVM_PROFILE_FILE='coverage-%p-%m.profraw' cargo test
grcov . -s . --binary-path ./target/debug/ -t html --branch --ignore-not-existing --excl-line "///" -o ./target/debug/coverage/
rm coverage*.profraw
rm default*.profraw
