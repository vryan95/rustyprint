# rustyprint native library

`rustyprint-ffi` is rustyprint as a shared library with a C ABI, for programs in C and other languages that can call C. The Ruby gem and the NuGet package are built on it. The fonts rustyprint needs are built in, so the library is a single file.

| Platform | Library |
| --- | --- |
| Linux | `librustyprint_ffi.so` |
| macOS | `librustyprint_ffi.dylib` |
| Windows | `rustyprint_ffi.dll`, with the import library `rustyprint_ffi.dll.lib` for linking |

The header is [include/rustyprint.h](include/rustyprint.h).

## Get it

Each release has an archive per platform, `rustyprint-native-<version>-<target>.tar.gz` (`.zip` for Windows), with the library, the header and this README. The targets are those of the command-line tool: `x86_64` and `aarch64` for `unknown-linux-gnu` (glibc 2.17 or newer), `unknown-linux-musl` (Alpine and other musl distributions), `apple-darwin` and `pc-windows-msvc`.

Or build it from the repository root, which puts it in `target/release/`:

```sh
cargo build --release -p rustyprint-ffi
```

## Use it

```c
#include <stdio.h>
#include <string.h>
#include "rustyprint.h"

int main(void) {
    const char *options = "{\"page_size\": \"A4\", \"margin\": \"15mm\"}";
    const char *html = "<link rel=\"stylesheet\" href=\"style.css\"><h1>Hello</h1>";
    const char *css = "h1 { color: navy }";

    rp_request *request = rp_request_new(options, strlen(options));
    rp_request_add_asset(request, "style.css", (const uint8_t *)css, strlen(css));

    rp_result *result = NULL;
    int ok = rp_render(request, html, strlen(html), &result) == RP_STATUS_OK;
    if (ok) {
        size_t len;
        const uint8_t *pdf = rp_result_pdf(result, &len);
        FILE *file = fopen("hello.pdf", "wb");
        fwrite(pdf, 1, len, file);
        fclose(file);
        printf("warnings: %s\n", rp_result_warnings_json(result, NULL));
    } else {
        fprintf(stderr, "%s\n", rp_result_error(result, NULL));
    }
    rp_result_free(result);
    rp_request_free(request);
    return ok ? 0 : 1;
}
```

Build it with GCC or Clang, with the library beside the program:

```sh
cc hello.c -I include -L . -lrustyprint_ffi -Wl,-rpath,'$ORIGIN' -o hello       # Linux
cc hello.c -I include -L . -lrustyprint_ffi -Wl,-rpath,@loader_path -o hello    # macOS
```

or with MSVC, keeping `rustyprint_ffi.dll` beside the program or on the `PATH`:

```bat
cl hello.c /I include rustyprint_ffi.dll.lib
```

- **Options** are JSON, in the format of [schema/render-options.schema.json](../../schema/render-options.schema.json), the same as the command-line tool's `--options` and the HTTP service's. An empty string gives the defaults. Invalid options still make a request, and rendering with it fails with `RP_STATUS_INVALID_OPTIONS`.
- **Assets** are the files the document refers to by relative URL: stylesheets, images, fonts, `iframe` documents. Add each under the path the document uses, such as `img/logo.png`. The library reads no files of its own and fetches nothing from the network unless the options allow hosts under `remote`.
- **Text** is UTF-8. Options and HTML are a pointer and a length and need not end in NUL; asset paths end in NUL. The strings the library gives end in NUL.
- **Memory** the library gives is the library's: free requests with `rp_request_free` and results with `rp_result_free`, which also frees the PDF, warnings and error the result holds. Both accept null.
- **Results:** `rp_render` gives a result whenever `out` is not null, even when it fails, so the error message can be read. Warnings, problems found in the document's CSS and assets, are a JSON array of strings.
- **Threads:** a request can render on several threads at once, as long as no thread adds assets to it meanwhile. Each render runs on a thread of its own with a large stack, so the calling thread's stack size does not matter.
- **Panics** never reach the caller: a bug in rustyprint gives `RP_STATUS_INTERNAL`.

| Status | Value | When |
| --- | --- | --- |
| `RP_STATUS_OK` | 0 | The render succeeded |
| `RP_STATUS_INVALID_ARGUMENT` | 1 | An argument is null where it may not be, or is not UTF-8 |
| `RP_STATUS_INVALID_OPTIONS` | 2 | The JSON options are not valid |
| `RP_STATUS_LIMIT_EXCEEDED` | 3 | The document exceeds a limit in the options |
| `RP_STATUS_TIMEOUT` | 4 | Rendering took longer than the deadline |
| `RP_STATUS_INTERNAL` | 5 | A bug in rustyprint |

## Develop it

The header is generated from [src/lib.rs](src/lib.rs) by cbindgen. After changing the ABI, regenerate it with `cargo run -p rustyprint-ffi --example header`; a test fails while it is out of date. `cargo test -p rustyprint-ffi` also builds [tests/smoke.c](tests/smoke.c) with the platform's C compiler and runs it.

This is the only crate allowed `unsafe` code, so it sets its own lints instead of the workspace's. Every `unsafe` block says why it is sound.
