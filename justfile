set shell := ["sh", "-eu", "-c"]

qt-version := "6.8.3"
build-dir := "build"

default:
    @just --list

bootstrap:
    rustup show
    cmake --version
    qmake -query QT_VERSION
    @echo "Required Qt: {{qt-version}}"

configure:
    cmake -S . -B {{build-dir}} -DCMAKE_BUILD_TYPE=Debug

build: configure
    cmake --build {{build-dir}} --parallel

run: build
    cmake --build {{build-dir}} --target run

format:
    cargo fmt --all
    cmake-format -i CMakeLists.txt app/cpp/CMakeLists.txt || true

format-check:
    cargo fmt --all --check
    cmake -S . -B {{build-dir}}/format-probe -DPARCHMINT_CONFIGURE_ONLY=ON

lint:
    cargo clippy --workspace --all-targets -- -D warnings
    cmake -S . -B {{build-dir}} -DCMAKE_BUILD_TYPE=Debug
    cmake --build {{build-dir}} --target qmllint

test: configure
    cargo test --workspace --exclude parchmint_bridge
    cmake --build {{build-dir}} --parallel
    ctest --test-dir {{build-dir}} --output-on-failure

test-rust:
    cargo test --workspace --exclude parchmint_bridge

build-rust:
    cargo build --workspace --exclude parchmint_bridge

smoke: build
    ctest --test-dir {{build-dir}} --output-on-failure -R parchmint-smoke

package-smoke: build
    cmake --install {{build-dir}} --prefix {{build-dir}}/install
    cmake -E tar cf {{build-dir}}/parchmint-smoke.tar --format=gnutar {{build-dir}}/install

release-evidence:
    cargo run -p parchmint-test-support --bin release-evidence -- release-evidence

fuzz-smoke:
    cargo run -p parchmint-test-support --release --bin fuzz-smoke -- 10000

bench-spikes:
    cargo test --locked -p parchmint-app -p parchmint-markdown -p parchmint-index -p parchmint-compile -p parchmint-storage --release -- --nocapture
