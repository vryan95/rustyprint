// How the pages show numbers.

export function pages (count) {
  return count === 1 ? '1 page' : `${count} pages`
}

// A size such as 512 Bytes, 45.3 KB or 1.2 MB.
export function size (bytes) {
  if (bytes < 1024) return `${bytes} Bytes`
  const kilobytes = bytes / 1024
  if (kilobytes < 1024) return `${kilobytes < 100 ? oneDecimal(kilobytes) : Math.round(kilobytes)} KB`
  return `${oneDecimal(kilobytes / 1024)} MB`
}

// 45.3, or 45 rather than 45.0.
function oneDecimal (value) {
  return String(Math.round(value * 10) / 10)
}

// How many pages a PDF has, by its page objects' /MediaBox entries.
export function pageCount (pdf) {
  return (pdf.toString('latin1').match(/\/MediaBox \[/g) ?? []).length
}

// The first page's width and height in millimetres, from its /MediaBox, or null.
export function pageMillimetres (pdf) {
  const box = /\/MediaBox \[0 0 ([\d.]+) ([\d.]+)\]/.exec(pdf.toString('latin1'))
  return box ? [millimetres(box[1]), millimetres(box[2])] : null
}

function millimetres (points) {
  return Math.round(Number(points) * 25.4 / 72)
}
