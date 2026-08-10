<p align="center">
  <img src="rust-client/resources/branding/xrtranslate-logo.png" alt="XRTranslate" width="120" />
</p>

<h1 align="center">XRTranslate</h1>

<p align="center">
  <strong>Real-Time Subtitles & Bidirectional Voice Translation for VRChat</strong>
</p>

<p align="center">
  <a href="README.md">中文</a> • <b>English</b>
</p>

<p align="center">
  <a href="#user-guide">User Guide</a> •
  <a href="#common-locations">Common Locations</a> •
  <a href="#citation">Citation</a> •
  <a href="#acknowledgements">Acknowledgements</a>
</p>

---

## User Guide

### 1. Download and Extract

Download the latest release package from [Releases](../../releases) and extract it to a fixed folder. Please keep the extracted directory intact, and do not move `config.json`, `resources/`, or `models/`.

### 2. Complete Setup via First-Run Onboarding

When launching for the first time, the client will guide you through setup:
* **Runtime Engine**: Automatically detect and download the optimal `llama.cpp` package for your PC in one click, or select an existing `llama-server.exe`.
* **Model Package Setup**: Easily download and verify model assets without manual file placement.

### 3. Start Translating

Return to the **Translation** page, select your microphone or system desktop audio, and click **Start Translation**. The client automatically manages local backend services when needed and shuts them down on exit.

---

## Common Locations

| Item | Default Path | Description |
| :--- | :--- | :--- |
| **Model Assets** | `models/` | Stores speech recognition and translation model packages |
| **Execution Logs** | `runtime/logs/` | Service & client execution logs |
| **Local Config** | `config.json` | Port numbers, model models, and rendering parameters |

### Default Model Resource Use

The figures below are for the default settings with both models running on the GPU. They can vary slightly with your GPU, llama.cpp version, and settings.

| Model | Purpose | File Size | Estimated VRAM Use |
| :--- | :--- | :--- | :--- |
| **Speech recognition model** | Speech recognition | About 1.8 GB | About 2.7 GB |
| **Translation model** | Translation | About 1.1 GB | About 1.4 GB |

Running both models together uses about **4.1 GB** of VRAM. An 8 GB or larger GPU is recommended to leave room for Windows and other applications.

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

Special thanks to the original [X-Translator](https://github.com/zhaoyx239/X-Translator) project and its authors for their contributions.

This project also uses or draws inspiration from [XTalk](https://github.com/xcc-zach/xtalk), [X-ASR](https://github.com/Gilgamesh-J/X-ASR), [Paraformer](https://github.com/modelscope/FunASR), [SenseVoice](https://github.com/FunAudioLLM/SenseVoice), [NiuTrans LMT](https://github.com/NiuTrans/NiuTrans.LMT), [Hunyuan-MT](https://github.com/Tencent-Hunyuan/Hunyuan-MT), [X-Voice](https://github.com/sunxy1997/X-Voice), [IndexTTS](https://github.com/index-tts/index-tts), and [OpenSTBench](https://github.com/sjtuyaj/OpenSTBench).

## License

This repository contains code released under different open-source licenses:

- Code originating from the original X-Translator project remains under the [MIT License](LICENSE-MIT).
- The native Rust client and newly added code are released under the [GNU Affero General Public License v3.0 (AGPL-3.0)](LICENSE).

Please refer to the corresponding license files and source files for the applicable licensing terms.
