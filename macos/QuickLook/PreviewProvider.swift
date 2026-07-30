import QuickLookUI
import UniformTypeIdentifiers

final class PreviewProvider: QLPreviewProvider, QLPreviewingController {
    func providePreview(
        for request: QLFilePreviewRequest,
        completionHandler handler: @escaping (QLPreviewReply?, Error?) -> Void
    ) {
        do {
            let preview = try ProjectPreview.load(from: request.fileURL)
            let reply = QLPreviewReply(
                dataOfContentType: .png,
                contentSize: preview.size
            ) { _ in
                preview.data
            }
            handler(reply, nil)
        } catch {
            handler(nil, error)
        }
    }
}
