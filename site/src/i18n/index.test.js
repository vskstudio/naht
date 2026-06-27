// @vitest-environment jsdom
import { describe, it, expect, beforeEach, vi } from 'vitest'
import { get } from 'svelte/store'

beforeEach(() => {
  localStorage.clear()
  vi.resetModules()
})

describe('detectLocale', () => {
  it('prefers a valid saved locale', async () => {
    localStorage.setItem('naht_locale', 'fr')
    const { detectLocale } = await import('./index.js')
    expect(detectLocale()).toBe('fr')
  })

  it('ignores an invalid saved locale and falls back to en', async () => {
    localStorage.setItem('naht_locale', 'zz')
    const { detectLocale } = await import('./index.js')
    expect(detectLocale()).toBe('en')
  })

  it('uses navigator.language prefix when nothing is saved', async () => {
    vi.spyOn(navigator, 'language', 'get').mockReturnValue('fr-FR')
    const { detectLocale } = await import('./index.js')
    expect(detectLocale()).toBe('fr')
  })
})

describe('setLocale', () => {
  it('updates the store and persists', async () => {
    const { locale, setLocale } = await import('./index.js')
    setLocale('fr')
    expect(get(locale)).toBe('fr')
    expect(localStorage.getItem('naht_locale')).toBe('fr')
    expect(document.documentElement.lang).toBe('fr')
  })

  it('rejects unknown codes', async () => {
    const { locale, setLocale } = await import('./index.js')
    const before = get(locale)
    setLocale('zz')
    expect(get(locale)).toBe(before)
  })
})

describe('t', () => {
  it('exposes the dictionary for the active locale', async () => {
    const { t, setLocale } = await import('./index.js')
    setLocale('en')
    expect(get(t).hero.ctaPrimary).toBeTypeOf('string')
    setLocale('fr')
    expect(get(t).hero.ctaPrimary).toBeTypeOf('string')
  })
})
