
# CrabIce

CrabIce is a lightweight HTTP API client written in [iced-rs](https://github.com/iced-rs/iced). It is designed to make testing and interacting with APIs simple, fast, and visually appealing.

## Features

- **Lightweight** – Minimal footprint and fast performance.
- **Request Management** – Save requests and open them later.
- **Automatic Video Playback** – If a response contains a video, it will play automatically.
- **Authentication Support** – Bearer token authentication built-in.
- **Flexible Content Types** – Supports `form-data`, `application/json`, and `application/x-www-form-urlencoded` for POST requests.
- **Customizable Themes** – Choose from predefined themes to suit your preference.

## Installation

Instructions for building and running the application:

```bash
# Clone the repository
git clone https://github.com/HimalPoudel334/CrabIce
# Navigate into the project
cd crabice

# Build the project
cargo build --release

# Run the application
cargo run --release
