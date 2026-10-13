/* Renders a document through the C ABI, as a C program using the library would.
 * Exits with 0 when every check passes. Built and run by tests/c_smoke.rs, and by the release
 * workflow against each library it builds. */

#include <stdio.h>
#include <string.h>

#include "rustyprint.h"

static int failures = 0;

#define CHECK(condition)                                                    \
    do {                                                                    \
        if (!(condition)) {                                                 \
            fprintf(stderr, "%s:%d: failed: %s\n", __FILE__, __LINE__,      \
                    #condition);                                            \
            failures++;                                                     \
        }                                                                   \
    } while (0)

static void renders_with_options_and_an_asset(void) {
    const char *options = "{\"page_size\": \"A5\", \"title\": \"Smoke test\"}";
    const char *html =
        "<link rel=\"stylesheet\" href=\"style.css\">"
        "<h1>Hello from C</h1><p>colr is not a property</p>";
    const char *css = "h1 { color: navy } p { colr: red }";

    rp_request *request = rp_request_new(options, strlen(options));
    CHECK(request != NULL);
    CHECK(rp_request_add_asset(request, "style.css", (const uint8_t *)css,
                               strlen(css)) == RP_STATUS_OK);

    rp_result *result = NULL;
    rp_status status = rp_render(request, html, strlen(html), &result);
    CHECK(status == RP_STATUS_OK);
    CHECK(result != NULL);
    CHECK(rp_result_status(result) == RP_STATUS_OK);

    size_t len = 0;
    const uint8_t *pdf = rp_result_pdf(result, &len);
    CHECK(pdf != NULL);
    CHECK(len > 5 && memcmp(pdf, "%PDF-", 5) == 0);

    const char *warnings = rp_result_warnings_json(result, &len);
    CHECK(warnings != NULL && warnings[0] == '[' && strlen(warnings) == len);
    CHECK(strstr(warnings, "colr") != NULL);
    CHECK(rp_result_error(result, NULL) == NULL);

    rp_result_free(result);
    rp_request_free(request);
}

static void reports_errors(void) {
    const char *options = "{\"page_size\": \"B99\"}";
    rp_request *request = rp_request_new(options, strlen(options));
    rp_result *result = NULL;
    rp_status status = rp_render(request, "<p>x</p>", 8, &result);
    CHECK(status == RP_STATUS_INVALID_OPTIONS);

    size_t len = 1;
    CHECK(rp_result_pdf(result, &len) == NULL && len == 0);
    const char *error = rp_result_error(result, &len);
    CHECK(error != NULL && strlen(error) == len && len > 0);
    CHECK(strcmp(rp_result_warnings_json(result, NULL), "[]") == 0);

    rp_result_free(result);
    rp_request_free(request);
}

int main(void) {
    printf("rustyprint %s\n", rp_version());
    renders_with_options_and_an_asset();
    reports_errors();
    rp_request_free(NULL);
    rp_result_free(NULL);
    if (failures > 0) {
        fprintf(stderr, "%d checks failed\n", failures);
        return 1;
    }
    printf("ok\n");
    return 0;
}
