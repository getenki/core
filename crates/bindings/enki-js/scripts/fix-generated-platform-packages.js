'use strict'

const fs = require('node:fs')
const path = require('node:path')

const repoRoot = path.resolve(__dirname, '..')
const npmDir = path.join(repoRoot, 'npm')

if (!fs.existsSync(npmDir)) {
  process.exit(0)
}

for (const entry of fs.readdirSync(npmDir, { withFileTypes: true })) {
  if (!entry.isDirectory()) {
    continue
  }

  const packageJsonPath = path.join(npmDir, entry.name, 'package.json')
  if (!fs.existsSync(packageJsonPath)) {
    continue
  }

  const packageJson = JSON.parse(fs.readFileSync(packageJsonPath, 'utf8'))
  const main = packageJson.main

  if (typeof main !== 'string' || main.length === 0) {
    continue
  }

  packageJson.exports = {
    '.': `./${main}`,
    './package.json': './package.json',
    ...(packageJson.exports && typeof packageJson.exports === 'object' ? packageJson.exports : {}),
  }

  fs.writeFileSync(packageJsonPath, `${JSON.stringify(packageJson, null, 2)}\n`)
}
