import { describe, it, expect } from 'vitest'
import en from './en.js'
import fr from './fr.js'

function leafPaths(obj, prefix = '') {
  const out = []
  for (const [k, v] of Object.entries(obj)) {
    const path = prefix ? `${prefix}.${k}` : k
    if (Array.isArray(v)) {
      out.push(`${path}[]:${v.length}`)
      v.forEach((item, i) => {
        if (item && typeof item === 'object') out.push(...leafPaths(item, `${path}[${i}]`))
      })
    } else if (v && typeof v === 'object') {
      out.push(...leafPaths(v, path))
    } else {
      out.push(path)
    }
  }
  return out.sort()
}

describe('dictionaries', () => {
  it('en and fr have identical key shape and array lengths', () => {
    expect(leafPaths(fr)).toEqual(leafPaths(en))
  })

  it('no value is an empty string', () => {
    const check = (obj) => {
      for (const v of Object.values(obj)) {
        if (typeof v === 'string') expect(v.length).toBeGreaterThan(0)
        else if (v && typeof v === 'object') check(v)
      }
    }
    check(en)
    check(fr)
  })
})
