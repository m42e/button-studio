import QuickLookThumbnailing

final class ThumbnailProvider: QLThumbnailProvider {
    override func provideThumbnail(
        for request: QLFileThumbnailRequest,
        _ handler: @escaping (QLThumbnailReply?, Error?) -> Void
    ) {
        do {
            let preview = try ProjectPreview.load(from: request.fileURL)
            let reply = QLThumbnailReply(imageFileURL: try preview.writeTemporaryImage())
            reply.extensionBadge = "buttonstudio"
            handler(reply, nil)
        } catch {
            handler(nil, error)
        }
    }
}
