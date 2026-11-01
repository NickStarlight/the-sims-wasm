import { vertexShader, fragmentShader } from './shaders.js'

// Integer textures retain the game's exact RGB565 expansion on the GPU.
export function createFrameRenderer(canvas) {
  let gl

  try {
    gl = canvas.getContext('webgl2', { antialias: false, premultipliedAlpha: false })
  } catch {}

  return gl?.createShader ? createWebGLRenderer(canvas, gl) : createCanvasRenderer(canvas)
}

function createCanvasRenderer(canvas) {
  const context = canvas.getContext('2d')

  return {
    packed: false,
    get info() {
      return { kind: 'canvas2d', width: canvas.width, height: canvas.height }
    },
    paint({ width, height, pixels, format = 32, pitch = width * 4 }) {
      let rgba

      if (format === 16) {
        rgba = new Uint8ClampedArray(width * height * 4)
        const view = new DataView(pixels.buffer, pixels.byteOffset, pixels.byteLength)

        for (let y = 0; y < height; y++)
          for (let x = 0; x < width; x++) {
            const color = view.getUint16(y * pitch + x * 2, true)
            const at = (y * width + x) * 4
            const r = color >>> 11
            const g = (color >>> 5) & 63
            const b = color & 31
            rgba.set(
              [
                Math.floor((r * 255) / 31),
                Math.floor((g * 255) / 63),
                Math.floor((b * 255) / 31),
                255,
              ],
              at,
            )
          }
      } else if (pitch === width * 4) {
        rgba = new Uint8ClampedArray(pixels.buffer, pixels.byteOffset, width * height * 4)
      } else {
        rgba = new Uint8ClampedArray(width * height * 4)

        for (let y = 0; y < height; y++)
          rgba.set(pixels.subarray(y * pitch, y * pitch + width * 4), y * width * 4)
      }

      context.putImageData(new ImageData(rgba, width, height), 0, 0)
    },
    clear() {
      context.clearRect(0, 0, canvas.width, canvas.height)
    },
  }
}

function createWebGLRenderer(canvas, gl) {
  return new WebGLRenderer(canvas, gl)
}

class WebGLRenderer {
  constructor(canvas, gl) {
    Object.assign(this, { canvas, gl })
    this.lastFrame = undefined
    this.program = undefined
    this.texture = undefined
    this.packedLocation = undefined
    this.heightLocation = undefined
    this.textureWidth = 0
    this.textureHeight = 0
    this.textureFormat = 0
    this.packed = true
    this.bindEvents()
  }

  bindEvents() {
    this.initialize()
    this.canvas.addEventListener('webglcontextlost', (event) => event.preventDefault())
    this.canvas.addEventListener('webglcontextrestored', () => {
      this.initialize()

      if (this.lastFrame) this.paint(this.lastFrame)
    })
  }

  shader = (type, source) => {
    const value = this.gl.createShader(type)
    this.gl.shaderSource(value, source)
    this.gl.compileShader(value)

    if (!this.gl.getShaderParameter(value, this.gl.COMPILE_STATUS))
      throw new Error(this.gl.getShaderInfoLog(value))

    return value
  }

  initialize = () => {
    const vertex = this.shader(this.gl.VERTEX_SHADER, vertexShader)
    const fragment = this.shader(this.gl.FRAGMENT_SHADER, fragmentShader)
    this.program = this.gl.createProgram()
    this.gl.attachShader(this.program, vertex)
    this.gl.attachShader(this.program, fragment)
    this.gl.linkProgram(this.program)
    this.gl.deleteShader(vertex)
    this.gl.deleteShader(fragment)

    if (!this.gl.getProgramParameter(this.program, this.gl.LINK_STATUS))
      throw new Error(this.gl.getProgramInfoLog(this.program))

    this.texture = this.gl.createTexture()
    this.gl.bindTexture(this.gl.TEXTURE_2D, this.texture)
    this.gl.texParameteri(this.gl.TEXTURE_2D, this.gl.TEXTURE_MIN_FILTER, this.gl.NEAREST)
    this.gl.texParameteri(this.gl.TEXTURE_2D, this.gl.TEXTURE_MAG_FILTER, this.gl.NEAREST)
    this.gl.texParameteri(this.gl.TEXTURE_2D, this.gl.TEXTURE_WRAP_S, this.gl.CLAMP_TO_EDGE)
    this.gl.texParameteri(this.gl.TEXTURE_2D, this.gl.TEXTURE_WRAP_T, this.gl.CLAMP_TO_EDGE)
    this.gl.disable(this.gl.DITHER)
    this.packedLocation = this.gl.getUniformLocation(this.program, 'packed')
    this.heightLocation = this.gl.getUniformLocation(this.program, 'imageHeight')
    this.textureWidth = this.textureHeight = this.textureFormat = 0
  }

  get info() {
    const debug = this.gl.getExtension('WEBGL_debug_renderer_info')

    return {
      kind: 'webgl2',
      vendor: this.gl.getParameter(debug ? debug.UNMASKED_VENDOR_WEBGL : this.gl.VENDOR),
      renderer: this.gl.getParameter(debug ? debug.UNMASKED_RENDERER_WEBGL : this.gl.RENDERER),
      width: this.canvas.width,
      height: this.canvas.height,
    }
  }

  paint = (frame) => {
    this.lastFrame = frame
    const { width, height, format = 32, pitch = width * 4 } = frame
    let { pixels } = frame

    if (this.gl.isContextLost()) return

    const packed = format === 16
    const bytes = packed ? 2 : 4

    if (
      pitch < width * bytes ||
      pitch % bytes ||
      pixels.byteLength < (height - 1) * pitch + width * bytes
    )
      throw new RangeError('Invalid frame layout')

    this.gl.viewport(0, 0, width, height)
    this.gl.useProgram(this.program)
    this.gl.bindTexture(this.gl.TEXTURE_2D, this.texture)
    this.gl.pixelStorei(this.gl.UNPACK_ALIGNMENT, 1)
    this.gl.pixelStorei(this.gl.UNPACK_ROW_LENGTH, pitch / bytes)
    let data = pixels

    if (packed) {
      if (pixels.byteOffset % 2) pixels = pixels.slice()

      data = new Uint16Array(pixels.buffer, pixels.byteOffset, Math.floor(pixels.byteLength / 2))
    }

    const internal = packed ? this.gl.R16UI : this.gl.RGBA8UI
    const layout = packed ? this.gl.RED_INTEGER : this.gl.RGBA_INTEGER
    const type = packed ? this.gl.UNSIGNED_SHORT : this.gl.UNSIGNED_BYTE

    if (
      width !== this.textureWidth ||
      height !== this.textureHeight ||
      format !== this.textureFormat
    ) {
      this.gl.texImage2D(this.gl.TEXTURE_2D, 0, internal, width, height, 0, layout, type, data)
      this.textureWidth = width
      this.textureHeight = height
      this.textureFormat = format
    } else this.gl.texSubImage2D(this.gl.TEXTURE_2D, 0, 0, 0, width, height, layout, type, data)

    this.gl.pixelStorei(this.gl.UNPACK_ROW_LENGTH, 0)
    this.gl.uniform1i(this.packedLocation, packed)
    this.gl.uniform1i(this.heightLocation, height)
    this.gl.drawArrays(this.gl.TRIANGLES, 0, 3)
  }

  clear = () => {
    this.lastFrame = undefined

    if (!this.gl.isContextLost()) {
      this.gl.clearColor(0, 0, 0, 0)
      this.gl.clear(this.gl.COLOR_BUFFER_BIT)
    }
  }
}
