/// <reference types="node" />

/** A page size as a width and height, each a CSS length such as `100mm`. */
export interface PageSizeDimensions {
  width: string
  height: string
}

/** Hosts that absolute URLs may be fetched from. Without them, nothing is fetched. */
export interface RemoteOptions {
  /** Host names, such as `cdn.example.com`, or `*.example.com` for its subdomains. */
  allowedHosts: string[]
  /** Also fetch plain http URLs. Default: false. */
  allowHttp?: boolean
  /** How long one fetch may take. Default: 10000. */
  timeoutMs?: number
  /** The largest file fetched. Default: 10 MB. */
  maxBytes?: number
  /** Redirects followed per fetch. Default: 5. */
  maxRedirects?: number
  /** Files fetched per render; the rest are not loaded. Default: 100. */
  maxFetches?: number
}

/** Bounds on the work of the render. Exceeding one fails the render, except that larger images are skipped with a warning. */
export interface Limits {
  /** Size of the HTML in bytes. Default: 20 MB. */
  maxInputBytes?: number
  /** DOM nodes across the document and its iframe documents. Default: 500000. */
  maxNodes?: number
  /** How deeply elements nest. Default: 512. */
  maxDepth?: number
  /** Pages in the PDF. Default: 2000. */
  maxPages?: number
  /** Width times height of an image. Default: 100000000. */
  maxImagePixels?: number
  /** How long the render may take, or null for no deadline. Default: 60000. */
  timeoutMs?: number | null
}

/** Asset contents: a string is encoded as UTF-8, bytes are used as they are. */
export type AssetData = string | ArrayBuffer | NodeJS.ArrayBufferView

/**
 * The options of schema/render-options.schema.json, with camelCase names, and the assets.
 * An option left out keeps its default.
 */
export interface RenderOptions {
  /** A3, A4 (the default), A5, Letter or Legal, in any case, or a width and height. */
  pageSize?: 'A3' | 'A4' | 'A5' | 'Letter' | 'Legal' | (string & {}) | PageSizeDimensions
  /** Turns the page so that it is wider than it is tall. Default: false. */
  landscape?: boolean
  /** Page margins like the CSS margin shorthand, such as `15mm` or `10mm 20mm`. Default: 2cm. */
  margin?: string
  /** The PDF's title. Default: the document's `<title>`. */
  title?: string
  /** A stylesheet applied after the document's own styles. */
  stylesheet?: string
  remote?: RemoteOptions
  limits?: Limits
  /**
   * The files the document refers to by relative or root-relative URL, by the path it uses
   * without a leading slash (`/css/app.css` is `css/app.css`).
   */
  assets?: Record<string, AssetData> | Map<string, AssetData>
}

export interface AsyncRenderOptions extends RenderOptions {
  /** Stops the render if it has not started yet. A render that has started runs to the end, bounded by `limits.timeoutMs`. */
  signal?: AbortSignal
}

export interface RenderOutput {
  /** The PDF. */
  pdf: Buffer
  /** Problems found in the document's CSS and assets, such as "invoice.css:12: unsupported property `colr`". */
  warnings: string[]
}

export interface RendererOptions {
  /** How many renders run at once, each on its own worker thread. Default: `os.availableParallelism()`. */
  concurrency?: number
}

export interface Renderer {
  /** Renders an HTML document to PDF on one of this renderer's workers. */
  render (html: string | Uint8Array | ArrayBuffer, options?: AsyncRenderOptions): Promise<RenderOutput>
  /** Stops the workers. Renders still waiting are refused. */
  close (): Promise<void>
}

/**
 * Renders an HTML document to PDF on a render worker. The event loop keeps running meanwhile,
 * and so do file system, DNS and crypto calls.
 */
export function render (html: string | Uint8Array | ArrayBuffer, options?: AsyncRenderOptions): Promise<RenderOutput>

/** Renders an HTML document to PDF on the calling thread, blocking the event loop until it is done. */
export function renderSync (html: string | Uint8Array | ArrayBuffer, options?: RenderOptions): RenderOutput

/** Makes a renderer with its own workers and concurrency. */
export function createRenderer (options?: RendererOptions): Renderer

/** The native library's version. */
export function nativeVersion (): string

/** The package's version, the same as the native library's. */
export const VERSION: string

/** A render that failed. Each kind of failure has its own subclass. */
export class RustyprintError extends Error {
  readonly kind: 'invalid_options' | 'limit_exceeded' | 'timeout' | 'internal'
}

/** The options are not valid, such as an unknown option or page size. */
export class InvalidOptionsError extends RustyprintError {
  readonly kind: 'invalid_options'
}

/** The document exceeds one of the limits in the options. */
export class LimitExceededError extends RustyprintError {
  readonly kind: 'limit_exceeded'
}

/** Rendering took longer than the deadline in the options. */
export class TimeoutError extends RustyprintError {
  readonly kind: 'timeout'
}

/** A bug in rustyprint. */
export class InternalError extends RustyprintError {
  readonly kind: 'internal'
}
