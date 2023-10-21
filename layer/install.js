import { mkdir, copyFile, writeFile } from 'fs/promises'
import { dirname, join } from 'path'
import { parseArgs } from 'util'

async function makeParents(path) {
  await mkdir(dirname(path), { recursive: true })
}

function parseArguments() {
  return parseArgs({
    options: {
      prefix: {
        type: 'string',
        default: '/usr',
      },
      destdir: {
        type: 'string',
        default: '',
      }
    }
  })
}

async function main() {
  const {values: {prefix, destdir}} = parseArguments()
  const libPath = join(prefix, 'liblatencyflex2_layer.so')
  const libPathDest = join(destdir, libPath)
  await makeParents(libPathDest)
  await copyFile('./liblatencyflex2_layer.so', libPathDest)
  const manifest = JSON.stringify({
    'file_format_version': '1.2.1',
    'layer': {
      'name': 'VK_LAYER_LFX_latencyflex2',
      'type': 'INSTANCE',
      'library_path': libPath,
      'library_arch': '64',
      'api_version': '1.3.268',
      'implementation_version': '2',
      'description': 'LatencyFleX (TM) latency reduction middleware',
      'functions': {},
      'instance_extensions': [],
      'device_extensions': [
        {
          'name': 'VK_NV_low_latency2',
          'spec_version': '1',
          'entrypoints': [
            'vkGetLatencyTimingsNV',
            'vkLatencySleepNV',
            'vkQueueNotifyOutOfBandNV',
            'vkSetLatencyMarkerNV',
            'vkSetLatencySleepModeNV',
          ],
        },
      ],
      'enable_environment': {
        'ENABLE_LAYER_LFX_latencyflex2': '1',
      },
      'disable_environment': {
        'DISABLE_LAYER_LFX_latencyflex2': '1',
      },
    },
  }, null, 4)
  const manifestPath = join(destdir, prefix, 'share/vulkan/implicit_layer.d/lfx2.json')
  await makeParents(manifestPath)
  await writeFile(manifestPath, manifest)
}

await main()