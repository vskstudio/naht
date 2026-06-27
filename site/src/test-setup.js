// Provide a deterministic in-memory localStorage for tests.
// jsdom's localStorage is unreliable under Node's experimental global of the
// same name, so we install our own before each test file runs.
class MemoryStorage {
  #map = new Map()
  getItem(k) { return this.#map.has(k) ? this.#map.get(k) : null }
  setItem(k, v) { this.#map.set(String(k), String(v)) }
  removeItem(k) { this.#map.delete(k) }
  clear() { this.#map.clear() }
}

const storage = new MemoryStorage()
Object.defineProperty(globalThis, 'localStorage', { value: storage, configurable: true, writable: true })
if (typeof window !== 'undefined') {
  Object.defineProperty(window, 'localStorage', { value: storage, configurable: true, writable: true })
}
