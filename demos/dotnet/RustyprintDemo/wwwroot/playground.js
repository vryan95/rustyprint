// Renders the playground form to PDF as it changes, and shows the PDF and the warnings.
const form = document.querySelector("[data-playground]")
const status = form.querySelector("[data-status]")
const warnings = form.querySelector("[data-warnings]")
const preview = form.querySelector("[data-preview]")
let timer
let latest = 0
let url

// Renders once typing pauses.
function queue() {
  clearTimeout(timer)
  timer = setTimeout(render, 600)
}

async function render(event) {
  event?.preventDefault()
  clearTimeout(timer)
  const request = ++latest
  status.textContent = "Rendering…"
  const response = await fetch(form.action, {
    method: "POST",
    body: new FormData(form),
    headers: { Accept: "application/json" }
  })
  const body = await response.json()
  // An older render that finishes late is dropped.
  if (request !== latest) return

  if (!response.ok) {
    status.textContent = body.error
    status.classList.add("error")
    return
  }
  status.classList.remove("error")
  const pages = `${body.pages} ${body.pages === 1 ? "page" : "pages"}`
  const [width, height] = body.page_mm
  status.textContent = `${pages} of ${width} × ${height} mm, rendered in ${body.milliseconds} ms`
  showWarnings(body.warnings)
  showPdf(body.pdf)
}

function showWarnings(list) {
  warnings.replaceChildren(...list.map((warning) => {
    const item = document.createElement("li")
    item.textContent = warning
    return item
  }))
  warnings.hidden = list.length === 0
}

function showPdf(base64) {
  const bytes = Uint8Array.from(atob(base64), (c) => c.charCodeAt(0))
  if (url) URL.revokeObjectURL(url)
  url = URL.createObjectURL(new Blob([bytes], { type: "application/pdf" }))
  preview.src = url
}

form.addEventListener("input", queue)
form.addEventListener("change", queue)
form.addEventListener("submit", render)
render()
