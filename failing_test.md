# Failing Test Analysis

## Comparison Results

| Repository | Test Status |
|------------|-------------|
| `~/dev/shoopdaloop` (master) | ✅ **All tests passed** (5841 assertions in 139 test cases) |
| `~/dev/shoopdaloop-2` (current branch) | ❌ **1 test fails** (5837 assertions passed, 1 failed in 138 test cases) |

## Failing Test Case

**Test Name:** `Chain - DryWet record MIDI basic`

**Location:** `src/backend/test/integration/test_chain_single_drywet_loop.cpp:479`

**Failure Details:**
```
REQUIRE( result_data.size() == 3 )
with expansion:
  35 == 3
```

The test expects 3 MIDI messages but is getting 35. This is the only failing test in the shoopdaloop-2 branch.

## Command to Run the Failing Test

```bash
./target/debug/build/backend-5ec9784d889b375a/out/cmake_build/build/test/test_runner "Chain - DryWet record MIDI basic"
```

## Notes

- The master branch (original C++ implementation) passes all 139 tests
- The current branch (Rust implementation) has 1 failing test in the integration tests
- All other MIDI-related unit tests pass (the MIDI unit test "Corner Case - note started during pre-play" that was failing earlier has been fixed)