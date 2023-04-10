CARGO_INCREMENTAL=0 RUSTFLAGS='-Cinstrument-coverage' LLVM_PROFILE_FILE='coverage-%p-%m.profraw' cargo test
grcov . -s ./src/ --binary-path ./target/debug/ -t html --branch --ignore-not-existing --excl-line "///" -o ./target/debug/coverage/
rm coverage*.profraw 2>/dev/null
rm default*.profraw 2>/dev/null
