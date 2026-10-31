// Integer textures retain the game's exact RGB565 expansion on the GPU.
export function createFrameRenderer(canvas) {
  let gl

  try {
    gl = canvas.getContext('webgl2', { antialias: false, premultipliedAlpha: false })
  } catch {}

  if (!gl?.createShader) {
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

  let lastFrame
  let program
  let texture
  let packedLocation
  let heightLocation
  let textureWidth = 0
  let textureHeight = 0
  let textureFormat = 0

  const shader = (type, source) => {
    const value = gl.createShader(type)
    gl.shaderSource(value, source)
    gl.compileShader(value)

    if (!gl.getShaderParameter(value, gl.COMPILE_STATUS))
      throw new Error(gl.getShaderInfoLog(value))

    return value
  }

  function initialize() {
    const vertex = shader(
      gl.VERTEX_SHADER,
      `#version 300 es
      void main() {
        vec2 p = vec2(float((gl_VertexID << 1) & 2), float(gl_VertexID & 2));
        gl_Position = vec4(p * 2.0 - 1.0, 0.0, 1.0);
      }`,
    )
    const fragment = shader(
      gl.FRAGMENT_SHADER,
      `#version 300 es
      precision highp float;
      precision highp int;
      uniform highp usampler2D image;
      uniform bool packed;
      uniform int imageHeight;
      out vec4 color;
      void main() {
        ivec2 p = ivec2(int(gl_FragCoord.x), imageHeight - 1 - int(gl_FragCoord.y));
        uvec4 pixel = texelFetch(image, p, 0);
        if (packed) {
          uint r = (pixel.r >> 11u) & 31u, g = (pixel.r >> 5u) & 63u, b = pixel.r & 31u;
          pixel = uvec4(r * 255u / 31u, g * 255u / 63u, b * 255u / 31u, 255u);
        }
        color = vec4(pixel) / 255.0;
      }`,
    )
    program = gl.createProgram()
    gl.attachShader(program, vertex)
    gl.attachShader(program, fragment)
    gl.linkProgram(program)
    gl.deleteShader(vertex)
    gl.deleteShader(fragment)

    if (!gl.getProgramParameter(program, gl.LINK_STATUS))
      throw new Error(gl.getProgramInfoLog(program))

    texture = gl.createTexture()
    gl.bindTexture(gl.TEXTURE_2D, texture)
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.NEAREST)
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.NEAREST)
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE)
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE)
    gl.disable(gl.DITHER)
    packedLocation = gl.getUniformLocation(program, 'packed')
    heightLocation = gl.getUniformLocation(program, 'imageHeight')
    textureWidth = textureHeight = textureFormat = 0
  }

  initialize()
  const renderer = {
    packed: true,
    get info() {
      const debug = gl.getExtension('WEBGL_debug_renderer_info')

      return {
        kind: 'webgl2',
        vendor: gl.getParameter(debug ? debug.UNMASKED_VENDOR_WEBGL : gl.VENDOR),
        renderer: gl.getParameter(debug ? debug.UNMASKED_RENDERER_WEBGL : gl.RENDERER),
        width: canvas.width,
        height: canvas.height,
      }
    },
    paint(frame) {
      lastFrame = frame
      const { width, height, format = 32, pitch = width * 4 } = frame
      let { pixels } = frame

      if (gl.isContextLost()) return

      const packed = format === 16
      const bytes = packed ? 2 : 4

      if (
        pitch < width * bytes ||
        pitch % bytes ||
        pixels.byteLength < (height - 1) * pitch + width * bytes
      )
        throw new RangeError('Invalid frame layout')

      gl.viewport(0, 0, width, height)
      gl.useProgram(program)
      gl.bindTexture(gl.TEXTURE_2D, texture)
      gl.pixelStorei(gl.UNPACK_ALIGNMENT, 1)
      gl.pixelStorei(gl.UNPACK_ROW_LENGTH, pitch / bytes)
      let data = pixels

      if (packed) {
        if (pixels.byteOffset % 2) pixels = pixels.slice()

        data = new Uint16Array(pixels.buffer, pixels.byteOffset, Math.floor(pixels.byteLength / 2))
      }

      const internal = packed ? gl.R16UI : gl.RGBA8UI
      const layout = packed ? gl.RED_INTEGER : gl.RGBA_INTEGER
      const type = packed ? gl.UNSIGNED_SHORT : gl.UNSIGNED_BYTE

      if (width !== textureWidth || height !== textureHeight || format !== textureFormat) {
        gl.texImage2D(gl.TEXTURE_2D, 0, internal, width, height, 0, layout, type, data)
        textureWidth = width
        textureHeight = height
        textureFormat = format
      } else gl.texSubImage2D(gl.TEXTURE_2D, 0, 0, 0, width, height, layout, type, data)

      gl.pixelStorei(gl.UNPACK_ROW_LENGTH, 0)
      gl.uniform1i(packedLocation, packed)
      gl.uniform1i(heightLocation, height)
      gl.drawArrays(gl.TRIANGLES, 0, 3)
    },
    clear() {
      lastFrame = undefined

      if (!gl.isContextLost()) {
        gl.clearColor(0, 0, 0, 0)
        gl.clear(gl.COLOR_BUFFER_BIT)
      }
    },
  }
  canvas.addEventListener('webglcontextlost', (event) => event.preventDefault())
  canvas.addEventListener('webglcontextrestored', () => {
    initialize()

    if (lastFrame) renderer.paint(lastFrame)
  })

  return renderer
}
