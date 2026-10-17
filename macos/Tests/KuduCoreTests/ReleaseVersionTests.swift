import Testing
@testable import KuduCore

@Suite struct ReleaseVersionTests {
    @Test func readsTags() {
        #expect(ReleaseVersion.fromTag("v0.1.4") == "0.1.4")
        #expect(ReleaseVersion.fromTag("0.2.10") == "0.2.10")
        #expect(ReleaseVersion.fromTag("latest") == nil)
        #expect(ReleaseVersion.fromTag("v1.0") == nil)
    }

    @Test func comparesNumerically() {
        #expect(ReleaseVersion.isNewer("0.1.4", than: "0.1.3"))
        #expect(ReleaseVersion.isNewer("0.1.10", than: "0.1.9"))
        #expect(ReleaseVersion.isNewer("1.0.0", than: "0.9.9"))
        #expect(!ReleaseVersion.isNewer("0.1.3", than: "0.1.3"))
        #expect(!ReleaseVersion.isNewer("0.1.2", than: "0.1.3"))
        #expect(!ReleaseVersion.isNewer("garbage", than: "0.1.3"))
    }
}
