<p align="center">
  <img src="rust-client/resources/branding/xrtranslate-logo.png" alt="XRTranslate" width="120" />
</p>

<h1 align="center">XRTranslate</h1>

<p align="center">
  <strong>为 VRChat 提供实时字幕与双向语音翻译</strong>
</p>

<p align="center">
  <b>中文</b> • <a href="README_EN.md">English</a>
</p>

<p align="center">
  <a href="#使用指南">使用指南</a> •
  <a href="#常用位置">常用位置</a> •
  <a href="#citation">Citation</a> •
  <a href="#acknowledgements">致谢</a>
</p>

---

## 使用指南

### 1. 下载并解压

从 [Releases](../../releases) 下载 XRTranslate 发布包并解压到固定位置。请保持解压后的目录完整，切勿随意移动 `config.json`、`resources/` 或 `models/` 目录。

### 2. 按首次启动引导完成配置

首次启动时，应用会开启极简引导：
* **运行时引擎**：可一键自动检测并下载适合当前电脑的 `llama.cpp`，或指定已有 `llama-server.exe`。
* **模型自动校验**：极简完成模型下载与准备，无需手动分拣解压。

### 3. 开始翻译

回到 **翻译** 页面，选择麦克风或系统声音，点击 **开始翻译** 即可。客户端会在需要时自动托管本地后台服务，并在退出时自动关闭。

---

## 常用位置

| 项目 | 默认路径 | 说明 |
| :--- | :--- | :--- |
| **模型文件** | `models/` | 放置语音识别模型与翻译模型等模型包 |
| **运行日志** | `runtime/logs/` | 查看后台服务与客户端日志 |
| **本地服务设置** | `config.json` | 端口、模型及渲染参数配置 |

### 默认模型的资源占用

以下为默认设置、两个模型均使用显卡运行时的参考值；不同显卡、llama.cpp 版本和设置会有少量差异。

| 模型 | 用途 | 文件大小 | 预计显存占用 |
| :--- | :--- | :--- | :--- |
| **语音识别模型** | 语音识别 | 约 1.8 GB | 约 2.7 GB |
| **翻译模型** | 翻译 | 约 1.1 GB | 约 1.4 GB |

两个模型同时运行时，预计占用约 **4.1 GB** 显存。建议使用 8 GB 或以上显存的显卡，以留出系统和其他程序所需空间。

---

## Citation

```bibtex
@misc{zhao2026xtranslatorrealtimemultilingualspeakeraware,
      title={X-Translator: A Real-Time Multilingual Speaker-Aware Speech-to-Speech Translation System},
      author={Yuxiang Zhao and Yichi Zhang and Yanjie An and Yanqiao Zhu and Zhanxun Liu and Yushen Chen and Qixi Zheng and Haina Zhu and Yunchong Xiao and Keqi Deng and Shuai Fan and Kai Yu and Xie Chen},
      year={2026},
      eprint={2607.17544},
      archivePrefix={arXiv},
      primaryClass={eess.AS},
      url={https://arxiv.org/abs/2607.17544},
}
```

---

## Acknowledgements

特别感谢原始项目 [X-Translator](https://github.com/zhaoyx239/X-Translator) 及其作者团队的卓越贡献。

同时感谢 [XTalk](https://github.com/xcc-zach/xtalk)、[X-ASR](https://github.com/Gilgamesh-J/X-ASR)、[Paraformer](https://github.com/modelscope/FunASR)、[SenseVoice](https://github.com/FunAudioLLM/SenseVoice)、[NiuTrans LMT](https://github.com/NiuTrans/LMT)、[Hunyuan-MT](https://github.com/Tencent-Hunyuan/Hunyuan-MT)、[X-Voice](https://github.com/sunnyxrxrx/X-Voice)、[IndexTTS](https://github.com/index-tts/index-tts) 与 [OpenSTBench](https://github.com/sjtuayj/OpenSTBench)。

## License

本项目包含采用不同开源许可证发布的代码：

- 原项目 X-Translator 相关代码沿用 [MIT License](LICENSE-MIT)。
- Rust 原生客户端及新增代码采用 [GNU Affero General Public License v3.0 (AGPL-3.0)](LICENSE)。

具体许可范围以仓库中的许可证文件及对应源码为准。
