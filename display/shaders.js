export const vertexShader = `#version 300 es
      void main() {
        vec2 p = vec2(float((gl_VertexID << 1) & 2), float(gl_VertexID & 2));
        gl_Position = vec4(p * 2.0 - 1.0, 0.0, 1.0);
      }`

export const fragmentShader = `#version 300 es
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
      }`
