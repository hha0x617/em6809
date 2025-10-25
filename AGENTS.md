# Repository Guidelines

## Project Structure & Modules
- `src/`: Core emulator code (CPU, bus/memory, I/O, CLI `main`).
- `include/` or `src/`: Public headers for core modules.
- `tests/`: Unit tests (e.g., `test_cpu_*.cpp`, `test_bus_*.cpp`).
- `assets/roms/`: Sample ROMs and test fixtures (do not commit proprietary ROMs).
- `docs/`: Notes, architecture sketches, and developer references.

## Build, Test, and Development
- CMake (preferred if `CMakeLists.txt` exists):
  - Configure: `cmake -S . -B build -DCMAKE_BUILD_TYPE=Release`
  - Build: `cmake --build build -j`
  - Test: `ctest --test-dir build -j`
- Make (if `Makefile` exists):
  - Build: `make -j`
  - Test: `make test`
- Run locally:
  - `./build/mc6809emu --rom assets/roms/example.rom --clock 1MHz` (adjust binary/flags per build output).

## Coding Style & Naming
- Indentation: 4 spaces, no tabs. Prefer C++17 (or C11 for C modules).
- Files: lowercase with underscores (e.g., `cpu_core.cpp`, `address_bus.h`).
- Names: `snake_case` for functions/variables; `PascalCase` for types; `UPPER_SNAKE_CASE` for constants/macros.
- Formatting: use repo `.clang-format` if present; otherwise `clang-format -i` on changed files.

## Testing Guidelines
- Framework: GoogleTest via CTest if configured; otherwise minimal unit tests in `tests/`.
- Naming: one component per file (e.g., `test_cpu_flags.cpp`).
- Coverage: add tests for new opcodes, addressing modes, and timing-sensitive paths.
- Run: `ctest --test-dir build -j` (or `make test`).

## Commit & Pull Request Guidelines
- Commits: use Conventional Commits (`feat:`, `fix:`, `test:`, `docs:`, `refactor:`). Imperative mood, concise subject, context in body.
- PRs: link related issues, summarize behavior changes, list test coverage, and include relevant screenshots or CLI output.
- Keep PRs small and focused; add notes on performance/timing changes to the CPU core.

## Security & Configuration Tips
- Do not commit licensed ROMs or firmware. Add unknown ROMs to `.gitignore`.
- Prefer relative paths and config flags over hardcoded absolute paths.
- Validate inputs: ROM size, entry points, and memory-mapped I/O ranges.

