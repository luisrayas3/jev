jev-go *ARGS:
    #!/usr/bin/env fish
    env (cat .env) cargo run --bin jev -- go "{{ARGS}}"

sync:
    cargo build

test: test-unit test-e2e

test-unit:
    cargo test --workspace

test-e2e:
    fish tests/e2e.fish

clean:
    cargo clean
    find plans -mindepth 1 -maxdepth 1 -type d -exec rm -rf {}/target \;

clean-all:
    cargo clean
    rm -rf plans/*/
    rm -rf logs/

deploy:
    @echo "stub"
