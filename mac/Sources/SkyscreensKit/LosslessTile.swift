import CoreVideo
import Foundation

public enum LosslessTile {
    public static func encode(_ buffer: CVPixelBuffer, rect: CGRect) -> Data? {
        let bounds = CGRect(x: 0, y: 0, width: CVPixelBufferGetWidth(buffer), height: CVPixelBufferGetHeight(buffer))
        let region = rect.integral.intersection(bounds)
        guard !region.isEmpty, let raw = PixelMemory.rows(of: buffer, in: region), !raw.isEmpty else { return nil }
        return PixelMemory.compressRawLZ4(raw)
    }
}
