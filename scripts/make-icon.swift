// Renders Lantern's app icon: a warm light in a dark field.
import Foundation
import AppKit

let size = 1024
let cs = CGColorSpaceCreateDeviceRGB()
let ctx = CGContext(data: nil, width: size, height: size, bitsPerComponent: 8,
                    bytesPerRow: 0, space: cs,
                    bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
let r = CGRect(x: 0, y: 0, width: size, height: size)

// Rounded-rect field.
let inset: CGFloat = 40
let path = CGPath(roundedRect: r.insetBy(dx: inset, dy: inset),
                  cornerWidth: 224, cornerHeight: 224, transform: nil)
ctx.addPath(path); ctx.clip()
ctx.setFillColor(CGColor(red: 0.055, green: 0.063, blue: 0.086, alpha: 1))
ctx.fill(r)

// Warm radial glow, offset slightly above centre.
let c = CGPoint(x: CGFloat(size) / 2, y: CGFloat(size) * 0.54)
let grad = CGGradient(colorsSpace: cs, colors: [
    CGColor(red: 1.00, green: 0.86, blue: 0.62, alpha: 1.00),
    CGColor(red: 0.98, green: 0.72, blue: 0.38, alpha: 0.55),
    CGColor(red: 0.85, green: 0.48, blue: 0.20, alpha: 0.12),
    CGColor(red: 0.05, green: 0.06, blue: 0.09, alpha: 0.00),
] as CFArray, locations: [0.0, 0.22, 0.48, 1.0])!
ctx.drawRadialGradient(grad, startCenter: c, startRadius: 0,
                       endCenter: c, endRadius: CGFloat(size) * 0.44,
                       options: [])

// Bright core.
ctx.setFillColor(CGColor(red: 1, green: 0.95, blue: 0.86, alpha: 0.95))
ctx.fillEllipse(in: CGRect(x: c.x - 62, y: c.y - 62, width: 124, height: 124))

let img = ctx.makeImage()!
let rep = NSBitmapImageRep(cgImage: img)
let data = rep.representation(using: .png, properties: [:])!
try! data.write(to: URL(fileURLWithPath: CommandLine.arguments[1]))
