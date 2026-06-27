import quickstart from '../../../../docs/quickstart.md?raw'
import architecture from '../../../../docs/architecture.md?raw'
import priorArt from '../../../../docs/prior-art.md?raw'
import spec from '../../../../docs/spec.md?raw'

export const docs = {
  quickstart: { title: 'Quickstart', raw: quickstart },
  architecture: { title: 'Architecture', raw: architecture },
  'prior-art': { title: 'Prior art', raw: priorArt },
  spec: { title: 'Spec', raw: spec },
}

export const sections = [
  { title: 'Getting Started', items: ['quickstart'] },
  { title: 'Concepts', items: ['architecture', 'prior-art'] },
  { title: 'Reference', items: ['spec'] },
]
