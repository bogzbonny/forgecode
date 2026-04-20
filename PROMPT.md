Refactor all sub-crates one by one and into forge_main. End this session each
time you refactor a single sub-crate, you will be called again in a loop until
all sub-crates are integrated into forge_main. The task is fully complete only
once all sub-crates are refactored into forge_main. Each time a sub-crate is
integrated into forge_main, ensure that the tests pass and the project builds
