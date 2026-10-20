# Sample documents

Documents the demos render, each an `index.html` with its stylesheet, images and fonts beside it, referred to by relative URL:

| Sample | Pages | Shows |
| --- | --- | --- |
| [invoice/](invoice/) | 3 | A logo, a 40-line table whose header row repeats on every page, totals, and page numbers in the page footer |
| [report/](report/) | 8 | A title page with its own page style, a table of contents with page numbers and leaders, headings, charts as images, tables, and page headers and footers |
| [letter/](letter/) | 1 | A one-page letter whose signature uses a web font, Caveat, from [letter/fonts/](letter/fonts/) |

Render one with the command-line tool, which resolves the relative URLs against the file's directory:

```sh
rustyprint demos/samples/report/index.html -o report.pdf
```

The Ruby, .NET and Node.js demos pass each sample's files to the renderer as assets, by the path the HTML uses.

The company, people and figures are made up. The fonts are under the SIL Open Font License: Noto, which rustyprint bundles, and Caveat ([letter/fonts/OFL.txt](letter/fonts/OFL.txt)). The logo and charts were drawn for these samples.
