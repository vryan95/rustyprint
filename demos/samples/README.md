# Sample documents

Documents the demos render, each an `index.html` with its stylesheet, images and fonts beside it, referred to by relative URL:

| Sample | Pages | Shows |
| --- | --- | --- |
| [invoice/](invoice/) | 3 | A logo, a 40-line table whose header row repeats on every page, totals, and page numbers in the page footer |
| [report/](report/) | 8 | A title page with its own page style, a table of contents with page numbers and leaders, headings, charts as images, tables, and page headers and footers |
| [report-ar/](report-ar/) | 6 | The report in Arabic, right to left: the bundled Arabic fonts, mirrored tables, lists and page margin boxes, Arabic-Indic page numbers, and Latin names within the Arabic text |
| [report-zh/](report-zh/) | 7 | The report in Simplified Chinese: the bundled Noto Sans SC, a chairman's letter set vertically, and page numbers as 第 1 页，共 7 页 |
| [emoji/](emoji/) | 14 | Every emoji of Unicode 18.0, 3,963 of them group by group, drawn in color with the bundled Noto Color Emoji: flags, skin tones and joined sequences such as families |
| [letter/](letter/) | 1 | A one-page letter whose signature uses a web font, Caveat, from [letter/fonts/](letter/fonts/) |

Render one with the command-line tool, which resolves the relative URLs against the file's directory:

```sh
rustyprint demos/samples/report/index.html -o report.pdf
```

The Ruby, .NET and Node.js demos pass each sample's files to the renderer as assets, by the path the HTML uses.

The company, people and figures are made up. The Arabic and Chinese reports and the emoji chart render with the fonts rustyprint bundles, without `@font-face`. The emoji chart lists the fully qualified emoji of Unicode's [emoji-test.txt](https://unicode.org/Public/emoji/latest/emoji-test.txt), version 18.0, in its groups and order. The fonts are under the SIL Open Font License: Noto, which rustyprint bundles, and Caveat ([letter/fonts/OFL.txt](letter/fonts/OFL.txt)). The logo and charts were drawn for these samples.
