# Scratchpad

## Iteration 1: Understanding the Project

### Objective
Refactor all sub-crates one by one into `forge_main`. Each iteration integrates one sub-crate.

### Current State
- `forge_main` depends on: forge_app, forge_api, forge_domain, forge_config, forge_walker, forge_display, forge_spinner, forge_select, forge_fs, forge_embed, forge_markdown_stream
- These are separate crates, not modules within `forge_main`
- Crates NOT yet in `forge_main`: forge_ci, forge_infra, forge_json_repair, forge_repo, forge_services, forge_snaps, forge_stream, forge_template, forge_test_kit, forge_tool_macros

### Integration Strategy
To move a sub-crate into `forge_main` without creating circular dependencies:
1. Move the code into `forge_main/src/` as a module
2. Update `forge_main` to use the local module
3. Update other crates that depend on the moved crate to import from `forge_main` instead
4. Remove the sub-crate from the workspace

**Circular dependency constraint**: Can only move crates that are NOT depended on by crates that `forge_main` already depends on (forge_app, forge_api, forge_domain, forge_config, etc.).

### Safe First Candidates
- `forge_snaps` (2 files, 258 lines): Only depended on by forge_repo, forge_infra, forge_services (none of which are in forge_main)
- `forge_ci` (24 files, 913 lines): Standalone, only used in its own tests

### Plan for Iteration 1
Integrate `forge_snaps` into `forge_main`:
1. Move `forge_snaps/src/lib.rs` and `forge_snaps/src/service.rs` into `forge_main/src/snapshots.rs`
2. Update `forge_main/src/lib.rs` to include the new module
3. Update `forge_repo/Cargo.toml` to add `forge_main` dependency and remove `forge_snaps` dependency
4. Update `forge_repo/src/fs_snap.rs` to import from `forge_main`
5. Remove `forge_snaps` dependency from `forge_infra/Cargo.toml` (unused)
6. Remove `forge_snaps` dependency from `forge_services/Cargo.toml` (unused)
7. Remove `forge_snaps` from workspace Cargo.toml
8. Run tests to verify

### Results - Iteration 1
**Task completed:** Integrated `forge_snaps` into `forge_main`

**Key finding:** Cannot have `forge_repo` depend on `forge_main` because of circular dependency:
- `forge_api` -> `forge_repo` -> `forge_main` -> `forge_api` (cycle!)

**Resolution:** Inlined `SnapshotService` directly into `forge_repo/src/fs_snap.rs` instead of having `forge_repo` depend on `forge_main`. This means:
- `forge_main` has the code as a module (`snapshots.rs`)
- `forge_repo` has its own copy (inlined)
- `forge_snaps` crate removed from workspace

**Tests:** All workspace tests pass (440 + 7 + 21 + 1 + 25 + 561 + 4 + 10 + 57 + 47 + 2 + 7 + 7 + 3 + 2 + 4 + 4 + 6 + 3 + 3 + 3 + 1 + 287 + 4 + 124 + 177 + 14 = ~2000+ tests)

**Next candidates for integration:**
- `forge_ci` - standalone, no other crate depends on it (but 24 files, 913 lines)
- `forge_stream` - used by forge_app, forge_api, forge_services (circular dep issue with forge_services)
- `forge_template` - used by forge_app, forge_domain, forge_repo (circular dep issue)
- `forge_json_repair` - used by forge_domain, forge_repo (circular dep issue)
- `forge_test_kit` - used by forge_app, forge_domain, forge_repo, forge_services (circular dep issue)
- `forge_tool_macros` - proc-macro crate, can't be inlined
