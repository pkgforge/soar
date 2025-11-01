# soar-dl Test Coverage Summary

This document summarizes the comprehensive unit tests added to the `soar-dl` crate.

## Test Files and Coverage

### 1. filter.rs (29 tests)
**Purpose**: Tests the `Filter` struct and its matching logic for filtering assets based on regex, glob patterns, include/exclude keywords.

**Test Categories**:
- **Basic functionality** (2 tests): Default filter, empty filter behavior
- **Regex matching** (2 tests): Single regex, multiple regexes
- **Glob matching** (6 tests): Case-sensitive/insensitive, multiple globs, wildcard patterns
- **Include keywords** (4 tests): Single/multiple keywords, alternatives, case-insensitive
- **Exclude keywords** (3 tests): Single/multiple keywords, alternatives
- **Combined filters** (1 test): All filter types working together
- **Edge cases** (5 tests): Empty keywords, whitespace handling, empty alternatives
- **Pattern matching** (2 tests): Wildcard patterns, question marks

**Key Test Scenarios**:
- Empty filter matches everything
- Regex patterns must all match
- At least one glob must match
- Include keywords (all groups must have matches)
- Exclude keywords (no group can have matches)
- Case-sensitive vs case-insensitive matching
- Comma-separated alternatives in keywords
- Whitespace trimming and empty string handling

### 2. utils.rs (21 tests)
**Purpose**: Tests utility functions for filename extraction and path resolution.

**Test Categories**:
- **filename_from_url** (7 tests): URL parsing, trailing slashes, percent encoding, query params, fragments
- **filename_from_header** (6 tests): Content-Disposition parsing, quotes, paths, multiple parameters
- **resolve_output_path** (8 tests): stdout, directories, explicit files, fallback logic

**Key Test Scenarios**:
- Extract filename from URL path segments
- Handle percent-encoded filenames
- Parse Content-Disposition headers
- Strip paths from filenames in headers
- Resolve output paths with various inputs (stdout "-", directories with "/", explicit paths)
- Prefer header filename over URL filename
- Error when no filename can be determined

### 3. types.rs (10 tests)
**Purpose**: Tests core type definitions: `Progress`, `OverwriteMode`, `ResumeInfo`.

**Test Categories**:
- **Progress enum** (4 tests): Starting, Chunk, Complete variants, cloning
- **OverwriteMode** (2 tests): Equality, cloning
- **ResumeInfo** (4 tests): With etag, with last_modified, serialization, cloning

**Key Test Scenarios**:
- Progress events track download state accurately
- OverwriteMode variants compare correctly
- ResumeInfo serializes/deserializes properly to JSON
- All types support required traits (Clone, Debug, etc.)

### 4. platform.rs (22 tests)
**Purpose**: Tests URL platform detection and parsing (GitHub, GitLab, OCI, Direct).

**Test Categories**:
- **OCI parsing** (2 tests): With/without https prefix
- **GitHub parsing** (6 tests): Various URL formats, tags, case-insensitive
- **GitLab parsing** (6 tests): Various URL formats, numeric projects, nested groups, special paths
- **Direct URL** (2 tests): HTTP/HTTPS direct downloads
- **Edge cases** (4 tests): Invalid URLs, special characters, percent encoding, quotes

**Key Test Scenarios**:
- Detect OCI references (ghcr.io/)
- Parse GitHub URLs (GitHub.com, GitHub:)
- Parse GitLab URLs (gitlab.com, numeric IDs, nested groups)
- Handle API paths and special routes as Direct URLs
- Extract project and tag information
- Percent-decode and strip quotes from tags
- Reject invalid URLs

### 5. oci.rs (15 tests)
**Purpose**: Tests OCI (Open Container Initiative) reference parsing and structures.

**Test Categories**:
- **OciReference parsing** (7 tests): Simple, with prefix, digest, no tag, nested packages
- **OciLayer** (3 tests): Title extraction, cloning
- **OciDownload** (3 tests): Builder pattern, parallel configuration
- **Deserialization** (3 tests): Manifest, layer, config JSON parsing

**Key Test Scenarios**:
- Parse OCI references with various formats
- Handle digest-based references (sha256:...)
- Default to "latest" tag when not specified
- Extract layer titles from annotations
- Builder pattern for download configuration
- Parallel download count clamping (minimum 1)
- Deserialize OCI manifest JSON structures

### 6. error.rs (12 tests)
**Purpose**: Tests error types and error message formatting.

**Test Categories**:
- **Error variants** (9 tests): Each error variant's display message
- **Error conversion** (1 test): ureq::Error to DownloadError
- **Error traits** (2 tests): Debug formatting, source chain

**Key Test Scenarios**:
- All error variants format correctly
- HTTP errors show status and URL
- NoMatch shows available assets
- IO errors wrap properly
- Network errors convert from ureq
- Error source chain is accessible

### 7. http_client.rs (16 tests)
**Purpose**: Tests HTTP client configuration and the shared agent.

**Test Categories**:
- **ClientConfig** (4 tests): Default values, building, timeout configuration, cloning
- **SharedAgent** (7 tests): All HTTP methods (GET, POST, PUT, DELETE, HEAD)
- **Configuration** (2 tests): Dynamic reconfiguration
- **Headers** (2 tests): Applying headers to requests
- **Traits** (1 test): Debug trait

**Key Test Scenarios**:
- Default configuration sets user agent
- Build agent with various configurations
- Create request builders for all HTTP methods
- Reconfigure shared client at runtime
- Apply custom headers to requests
- Clone shared agent instances

## Testing Strategy

### Pure Functions
Focus on testing pure functions that don't require external dependencies:
- String parsing and transformation (utils, platform)
- Pattern matching logic (filter)
- Type construction and serialization (types, oci, error)
- Configuration and builder patterns (http_client, oci)

### Test Coverage Metrics
- **Total Tests**: 125 unit tests
- **Files with Tests**: 7 out of 16 source files (43.75%)
- **Test Focus**: Pure functions, data structures, parsing logic

### Files Not Requiring Unit Tests
The following files are better suited for integration tests or don't need unit tests:
- **download.rs**: Requires HTTP server, file system, complex integration
- **http.rs**: Thin wrapper around http_client, tested through integration
- **github.rs/gitlab.rs**: Require API mocking, trait implementations tested indirectly
- **release.rs**: Orchestration logic, tested through integration
- **traits.rs**: Trait definitions only, no implementation
- **xattr.rs**: File system dependent, requires real files
- **lib.rs**: Module declarations only

## Test Execution

Run all tests:
```bash
cargo test --package soar-dl
```

Run specific test module:
```bash
cargo test --package soar-dl filter::tests
cargo test --package soar-dl utils::tests
```

Run with output:
```bash
cargo test --package soar-dl -- --nocapture
```

## Future Test Improvements

1. **Integration Tests**: Add tests that exercise download.rs, http.rs, release.rs with mock servers
2. **Property-Based Testing**: Use proptest for filter matching logic
3. **Benchmark Tests**: Performance tests for large file downloads
4. **Error Recovery**: Test resume functionality with simulated failures
5. **Concurrency Tests**: Parallel download stress tests

## Test Conventions

All tests follow these conventions:
- Use descriptive test names: `test_<module>_<scenario>`
- Group related tests together
- Test happy paths, edge cases, and error conditions
- Use assert macros with clear failure messages
- Keep tests isolated and deterministic
- Follow existing project test patterns (from soar-utils)