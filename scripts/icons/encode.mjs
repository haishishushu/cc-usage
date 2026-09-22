/**
 * 最小 PNG / ICO 编码器，只依赖 Node 内置 zlib。
 * 不引第三方图像库是有意的：图标是构建产物，多一个原生依赖就多一份装不上的风险。
 */
import zlib from "node:zlib"

const CRC_TABLE = (() => {
  const table = new Int32Array(256)
  for (let n = 0; n < 256; n += 1) {
    let c = n
    for (let k = 0; k < 8; k += 1) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1
    table[n] = c
  }
  return table
})()

function crc32(buf) {
  let c = -1
  for (let i = 0; i < buf.length; i += 1) c = CRC_TABLE[(c ^ buf[i]) & 0xff] ^ (c >>> 8)
  return (c ^ -1) >>> 0
}

function chunk(type, data) {
  const head = Buffer.alloc(4)
  head.writeUInt32BE(data.length, 0)
  const body = Buffer.concat([Buffer.from(type, "latin1"), data])
  const crc = Buffer.alloc(4)
  crc.writeUInt32BE(crc32(body), 0)
  return Buffer.concat([head, body, crc])
}

/** 32 位 RGBA 的 PNG。逐行 filter 全用 0（None）——图标色块大，压缩率已经足够。 */
export function encodePNG(size, rgba) {
  const stride = size * 4
  const raw = Buffer.alloc((stride + 1) * size)
  for (let y = 0; y < size; y += 1) {
    raw[y * (stride + 1)] = 0
    rgba.copy(raw, y * (stride + 1) + 1, y * stride, (y + 1) * stride)
  }
  const ihdr = Buffer.alloc(13)
  ihdr.writeUInt32BE(size, 0)
  ihdr.writeUInt32BE(size, 4)
  ihdr[8] = 8      // bit depth
  ihdr[9] = 6      // color type: RGBA
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk("IHDR", ihdr),
    chunk("IDAT", zlib.deflateSync(raw, { level: 9 })),
    chunk("IEND", Buffer.alloc(0)),
  ])
}

/**
 * 多尺寸 ICO，每张子图存为 PNG（Vista 起的标准做法，Windows 任务栏与资源管理器都认）。
 * 尺寸给得越全，系统在各种 DPI 下越不需要自己缩放——这正是任务栏图标发虚的根因。
 */
export function encodeICO(entries) {
  const header = Buffer.alloc(6)
  header.writeUInt16LE(0, 0)
  header.writeUInt16LE(1, 2)
  header.writeUInt16LE(entries.length, 4)

  const dir = Buffer.alloc(16 * entries.length)
  let offset = header.length + dir.length
  for (let i = 0; i < entries.length; i += 1) {
    const { size, png } = entries[i]
    const o = i * 16
    dir[o] = size >= 256 ? 0 : size      // 256 在 ICO 里用 0 表示
    dir[o + 1] = size >= 256 ? 0 : size
    dir[o + 2] = 0                        // 调色板颜色数
    dir[o + 3] = 0
    dir.writeUInt16LE(1, o + 4)           // color planes
    dir.writeUInt16LE(32, o + 6)          // bits per pixel
    dir.writeUInt32LE(png.length, o + 8)
    dir.writeUInt32LE(offset, o + 12)
    offset += png.length
  }
  return Buffer.concat([header, dir, ...entries.map((e) => e.png)])
}
