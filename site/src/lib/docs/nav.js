import quickstart from '../../../../docs/quickstart.md?raw'
import architecture from '../../../../docs/architecture.md?raw'
import priorArt from '../../../../docs/prior-art.md?raw'

export const docs = {
  quickstart: { title: 'Quickstart', raw: quickstart },
  architecture: { title: 'Architecture', raw: architecture },
  'prior-art': { title: 'Prior art', raw: priorArt },
}

export const sections = [
  { title: 'Getting Started', items: ['quickstart'] },
  { title: 'Concepts', items: ['architecture', 'prior-art'] },
]
