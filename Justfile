sync:
    cargo build

test: test-unit test-e2e

test-unit:
    cargo test --workspace

test-e2e:
    bash tests/e2e.sh

clean:
    cargo clean
    find plans -mindepth 1 -maxdepth 1 -type d -exec rm -rf {}/target \;

clean-all:
    cargo clean
    rm -rf plans/*/
    rm -rf logs/

deploy:
    @echo "stub"
