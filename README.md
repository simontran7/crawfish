<div align="center">
  <img width="170px" src="docs/logo.png">
  <h1>crawfish</h1>
  <p>simple programming language for the layman</p>
</div>

> [!CAUTION]
> The compiler can't compile crawfish programs yet.

## Installation (Building from Source)

### Dependencies

- Rust Compiler
- LLVM

### Steps

1. Install Rust
```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

2. Install `llvm-config-22` and `libpolly-22-dev`
```sh
wget https://apt.llvm.org/llvm.sh
chmod +x llvm.sh
sudo ./llvm.sh 22
sudo apt install libpolly-22-dev
```

3. Git clone the repository

```sh
git clone https://github.com/simontran7/crawfish.git
```

4. `cd` into the `crawfish/` directory, then build the project with GNU Make

```sh
cd crawfish/
cargo build --release
```

5. Move the `target/release/crawfish` binary to a desired location (e.g. in `/Users/<your name>`), then add it to your `PATH` by adding the following line to your `.bashrc` file

```sh
# in your .bashrc
export PATH=$PATH:<path to the crawfish compiler executable>
```

## Usage

```
crawfish compile <file.crw 
```