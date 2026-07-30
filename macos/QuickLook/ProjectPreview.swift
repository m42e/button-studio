import CoreGraphics
import Foundation
import ImageIO

enum ProjectPreviewError: LocalizedError {
    case invalidProject
    case missingPreview
    case invalidPreview

    var errorDescription: String? {
        switch self {
        case .invalidProject:
            return "The Button Studio project is invalid."
        case .missingPreview:
            return "This project does not contain a preview. Open and save it in Button Studio to add one."
        case .invalidPreview:
            return "The embedded project preview is invalid."
        }
    }
}

struct ProjectPreview {
    let data: Data
    let image: CGImage

    var size: CGSize {
        CGSize(width: image.width, height: image.height)
    }

    static func load(from url: URL) throws -> ProjectPreview {
        let projectData = try Data(contentsOf: url, options: .mappedIfSafe)
        guard
            let project = try JSONSerialization.jsonObject(with: projectData) as? [String: Any]
        else {
            throw ProjectPreviewError.invalidProject
        }
        guard
            let preview = project["preview"] as? [String: Any],
            preview["format"] as? String == "png",
            let encoded = preview["data"] as? String
        else {
            throw ProjectPreviewError.missingPreview
        }
        guard
            let data = Data(base64Encoded: encoded),
            let source = CGImageSourceCreateWithData(data as CFData, nil),
            let image = CGImageSourceCreateImageAtIndex(source, 0, nil)
        else {
            throw ProjectPreviewError.invalidPreview
        }
        return ProjectPreview(data: data, image: image)
    }

    func writeTemporaryImage() throws -> URL {
        let url = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString)
            .appendingPathExtension("png")
        try data.write(to: url, options: .atomic)
        return url
    }
}
