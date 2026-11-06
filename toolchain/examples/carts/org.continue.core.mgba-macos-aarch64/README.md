# mGBA Core Cart

这个 Cart 示例展示单个 Core 资源如何被打包为面向 macOS aarch64 的 CartArtifact。

同一个 CartRelease 可以另有 Windows 或 iOS CartArtifact；那些会是其他 `.cart` 文件，拥有自己的 artifact target 和 bundle checksum。
