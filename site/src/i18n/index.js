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

// Per-locale document metadata, so a FR visitor gets FR <title>/description and
// the correct og:locale even though both languages share one SPA URL.
const META = {
  en: {
    title: 'Naht — bidirectional, conflict-safe Roblox ⇄ filesystem sync',
    description:
      'Naht is the seam between your filesystem and Roblox Studio: bidirectional, conflict-safe, never-destructive sync with a real 3-way merge and persisted state.',
    ogLocale: 'en_US',
  },
  fr: {
    title: 'Naht — synchronisation Roblox ⇄ système de fichiers, bidirectionnelle et sans conflit',
    description:
      'Naht est la couture entre votre système de fichiers et Roblox Studio : une synchronisation bidirectionnelle, sans conflit et non destructive, avec une vraie fusion à 3 voies et un état persistant.',
    ogLocale: 'fr_FR',
  },
}

function applyMeta(code) {
  if (typeof document === 'undefined') return
  const m = META[code] || META.en
  document.title = m.title
  const set = (selector, value) => {
    const el = document.head.querySelector(selector)
    if (el) el.setAttribute('content', value)
  }
  set('meta[name="description"]', m.description)
  set('meta[property="og:title"]', m.title)
  set('meta[property="og:description"]', m.description)
  set('meta[property="og:locale"]', m.ogLocale)
  set('meta[name="twitter:title"]', m.title)
  set('meta[name="twitter:description"]', m.description)
}

locale.subscribe((code) => {
  if (typeof localStorage !== 'undefined') localStorage.setItem(STORAGE_KEY, code)
  if (typeof document !== 'undefined') document.documentElement.lang = code
  applyMeta(code)
})

export const t = derived(locale, ($locale) => dicts[$locale])
