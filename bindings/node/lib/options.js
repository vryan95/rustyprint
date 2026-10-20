'use strict'

// The options as the JSON of schema/render-options.schema.json: camelCase names such as
// `pageSize` become `page_size`, and names already in snake_case stay as they are.
function snakeCase (value) {
  if (Array.isArray(value)) return value.map(snakeCase)
  if (value === null || typeof value !== 'object') return value
  return Object.fromEntries(Object.entries(value).map(([key, item]) =>
    [key.replace(/[A-Z]/g, letter => `_${letter.toLowerCase()}`), snakeCase(item)]))
}

module.exports = { snakeCase }
