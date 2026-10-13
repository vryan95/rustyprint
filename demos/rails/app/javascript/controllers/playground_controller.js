import { Controller } from "@hotwired/stimulus"

// Renders the playground form to PDF as it changes, and shows the PDF and the warnings.
export default class extends Controller {
  static targets = ["preview", "status", "warnings"]

  connect() {
    this.render()
  }

  disconnect() {
    clearTimeout(this.timer)
    if (this.url) URL.revokeObjectURL(this.url)
  }

  // Renders once typing pauses.
  queue() {
    clearTimeout(this.timer)
    this.timer = setTimeout(() => this.render(), 600)
  }

  async render(event) {
    event?.preventDefault()
    clearTimeout(this.timer)
    const request = (this.request = (this.request || 0) + 1)
    this.statusTarget.textContent = "Rendering…"
    const response = await fetch(this.element.action, {
      method: "POST",
      body: new FormData(this.element),
      headers: { Accept: "application/json" }
    })
    const body = await response.json()
    // An older render that finishes late is dropped.
    if (request !== this.request) return

    if (!response.ok) {
      this.statusTarget.textContent = body.error
      this.statusTarget.classList.add("error")
      return
    }
    this.statusTarget.classList.remove("error")
    const pages = `${body.pages} ${body.pages === 1 ? "page" : "pages"}`
    const [width, height] = body.page_mm
    this.statusTarget.textContent = `${pages} of ${width} × ${height} mm, rendered in ${body.milliseconds} ms`
    this.showWarnings(body.warnings)
    this.showPdf(body.pdf)
  }

  showWarnings(warnings) {
    this.warningsTarget.replaceChildren(...warnings.map((warning) => {
      const item = document.createElement("li")
      item.textContent = warning
      return item
    }))
    this.warningsTarget.hidden = warnings.length === 0
  }

  showPdf(base64) {
    const bytes = Uint8Array.from(atob(base64), (c) => c.charCodeAt(0))
    if (this.url) URL.revokeObjectURL(this.url)
    this.url = URL.createObjectURL(new Blob([bytes], { type: "application/pdf" }))
    this.previewTarget.src = this.url
  }
}
