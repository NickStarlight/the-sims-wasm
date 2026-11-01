import stylistic from '@stylistic/eslint-plugin'
import globals from 'globals'

export default [
  {
    files: ['web/**/*.js'],
    plugins: { '@stylistic': stylistic },
    languageOptions: {
      globals: { ...globals.browser, ...globals.worker },
    },
    rules: {
      'no-undef': 'error',
      'no-unused-vars': ['error', { args: 'none', caughtErrors: 'none' }],
      'no-dupe-class-members': 'error',
      'no-dupe-keys': 'error',
      'prefer-const': 'error',
      'no-var': 'error',
      'one-var': ['error', 'never'],
      '@stylistic/semi': ['error', 'never'],
      '@stylistic/padding-line-between-statements': [
        'error',
        { blankLine: 'always', prev: '*', next: 'return' },
        { blankLine: 'always', prev: '*', next: 'block-like' },
        { blankLine: 'always', prev: 'block-like', next: '*' },
        { blankLine: 'always', prev: '*', next: ['if', 'for', 'while', 'switch', 'try'] },
        { blankLine: 'always', prev: ['if', 'for', 'while', 'switch', 'try'], next: '*' },
        { blankLine: 'always', prev: 'import', next: '*' },
        { blankLine: 'any', prev: 'import', next: 'import' },
      ],
      '@stylistic/lines-between-class-members': ['error', 'always'],
      'max-lines-per-function': ['error', { max: 70, skipBlankLines: true, skipComments: true }],
    },
  },
]
