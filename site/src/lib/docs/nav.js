// Documentation structure: ordered sections and their pages. Page titles + descriptions
// come from each .svx file's frontmatter (per locale); here we keep only the locale-independent
// shape — icon, accent colour, and reading order. Section titles are translated below.

export const sections = [
  {
    id: 'start',
    icon: 'rocket',
    items: [
      { slug: 'introduction', icon: 'sparkles', accent: 'fs' },
      { slug: 'quickstart', icon: 'rocket', accent: 'fs' },
      { slug: 'installation', icon: 'download', accent: 'fs' },
    ],
  },
  {
    id: 'concepts',
    icon: 'blocks',
    items: [
      { slug: 'core-concepts', icon: 'waypoints', accent: 'studio' },
      { slug: 'architecture', icon: 'blocks', accent: 'studio' },
      { slug: 'sync-engine', icon: 'route', accent: 'studio' },
      { slug: 'conflicts', icon: 'merge', accent: 'merge' },
    ],
  },
  {
    id: 'config',
    icon: 'settings',
    items: [
      { slug: 'file-mapping', icon: 'folderTree', accent: 'fs' },
      { slug: 'configuration', icon: 'settings', accent: 'studio' },
      { slug: 'migrating', icon: 'swap', accent: 'merge' },
    ],
  },
  {
    id: 'reference',
    icon: 'terminal',
    items: [
      { slug: 'cli', icon: 'terminal', accent: 'fs' },
      { slug: 'protocol', icon: 'network', accent: 'studio' },
      { slug: 'terrain-assets', icon: 'mountain', accent: 'merge' },
    ],
  },
  {
    id: 'project',
    icon: 'book',
    items: [
      { slug: 'troubleshooting', icon: 'lifeBuoy', accent: 'fs' },
    ],
  },
]

export const sectionTitles = {
  en: {
    start: 'Getting Started',
    concepts: 'Core Concepts',
    config: 'Configuration',
    reference: 'Reference',
    project: 'Project',
  },
  fr: {
    start: 'Démarrer',
    concepts: 'Concepts',
    config: 'Configuration',
    reference: 'Référence',
    project: 'Projet',
  },
}

// Flat reading order for prev/next + lookups.
export const order = sections.flatMap((s) => s.items.map((i) => i.slug))

// slug -> { icon, accent }
export const meta = Object.fromEntries(
  sections.flatMap((s) => s.items.map((i) => [i.slug, { icon: i.icon, accent: i.accent }])),
)

export const DEFAULT_SLUG = 'introduction'
