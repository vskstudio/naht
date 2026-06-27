import { writable, derived } from 'svelte/store'
import en from './en.js'
import fr from './fr.js'

const dicts = { en, fr }
export const LOCALES = ['en', 'fr']
const STORAGE_KEY = 'naht_locale'

export function detectLocale() {
  if (typeof localStorage !== 'undefined') {
    const saved = localStorage.getItem(STORAGE_KEY)
    if (saved && LOCALES.includes(saved)) return saved
  }
  if (typeof navigator !== 'undefined' && navigator.language) {
    const prefix = navigator.language.slice(0, 2).toLowerCase()
    if (LOCALES.includes(prefix)) return prefix
  }
  return 'en'
}

export const locale = writable(detectLocale())

export function setLocale(code) {
  if (LOCALES.includes(code)) locale.set(code)
}

locale.subscribe((code) => {
  if (typeof localStorage !== 'undefined') localStorage.setItem(STORAGE_KEY, code)
  if (typeof document !== 'undefined') document.documentElement.lang = code
})

export const t = derived(locale, ($locale) => dicts[$locale])
