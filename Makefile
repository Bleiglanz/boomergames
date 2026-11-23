.PHONY: check fmt clippy test wasm build dev determinism clean

check: fmt clippy

fmt:
	cargo fmt --all -- --check

clippy:
	cargo clippy --workspace --all-targets --all-features -- -D warnings
	cargo clippy --target wasm32-unknown-unknown -p boomer-domain -p boomer-protocol -p boomer-client -- -D warnings

test:
	cargo test --workspace

wasm:
	wasm-pack build crates/boomer-client --target web --release --out-dir ../../web/src/wasm --out-name boomer_client

wasm-dev:
	wasm-pack build crates/boomer-client --target web --dev --out-dir ../../web/src/wasm --out-name boomer_client

build: wasm
	cd web && npm run typecheck && npm run build

dev: wasm-dev
	cd web && npm run dev

determinism:
	cargo test -p boomer-domain
	wasm-pack test --node crates/boomer-domain

clean:
	cargo clean
	rm -rf web/dist web/src/wasm
