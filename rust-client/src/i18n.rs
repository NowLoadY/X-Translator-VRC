//! Centralized UI copy and translations.
//!
//! UI code always uses the English source text as a key. Adding a language is
//! a single consolidated table addition rather than separate arrays per language.
use serde::{Deserialize, Serialize};
use std::borrow::Cow;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UiLanguage {
    #[default]
    English,
    Chinese,
    Japanese,
    Korean,
    Russian,
}

impl UiLanguage {
    pub const ALL: [Self; 5] = [
        Self::English,
        Self::Chinese,
        Self::Japanese,
        Self::Korean,
        Self::Russian,
    ];

    pub const fn display_name(self) -> &'static str {
        match self {
            Self::English => "English",
            Self::Chinese => "中文",
            Self::Japanese => "日本語",
            Self::Korean => "한국어",
            Self::Russian => "Русский",
        }
    }
}

/// Looks up a fixed UI string from the consolidated multi-language dictionary.
/// Missing translations intentionally fall back to the English source key.
pub fn tr(language: UiLanguage, english: &'static str) -> &'static str {
    match language {
        UiLanguage::English => english,
        UiLanguage::Chinese => DICTIONARY
            .iter()
            .find_map(|(key, zh, _ja, _ko, _ru)| (*key == english).then_some(*zh))
            .unwrap_or(english),
        UiLanguage::Japanese => DICTIONARY
            .iter()
            .find_map(|(key, _zh, ja, _ko, _ru)| (*key == english).then_some(*ja))
            .unwrap_or(english),
        UiLanguage::Korean => DICTIONARY
            .iter()
            .find_map(|(key, _zh, _ja, ko, _ru)| (*key == english).then_some(*ko))
            .unwrap_or(english),
        UiLanguage::Russian => DICTIONARY
            .iter()
            .find_map(|(key, _zh, _ja, _ko, ru)| (*key == english).then_some(*ru))
            .unwrap_or(english),
    }
}

/// Dynamic counterpart for status text that originates outside the UI layer.
pub fn tr_dynamic<'a>(language: UiLanguage, english: &'a str) -> Cow<'a, str> {
    match language {
        UiLanguage::English => Cow::Borrowed(english),
        UiLanguage::Chinese => {
            if let Some((_, zh, _, _, _)) = DICTIONARY.iter().find(|(key, _, _, _, _)| *key == english) {
                Cow::Borrowed(zh)
            } else {
                Cow::Borrowed(english)
            }
        }
        UiLanguage::Japanese => {
            if let Some((_, _, ja, _, _)) = DICTIONARY.iter().find(|(key, _, _, _, _)| *key == english) {
                Cow::Borrowed(ja)
            } else {
                Cow::Borrowed(english)
            }
        }
        UiLanguage::Korean => {
            if let Some((_, _, _, ko, _)) = DICTIONARY.iter().find(|(key, _, _, _, _)| *key == english) {
                Cow::Borrowed(ko)
            } else {
                Cow::Borrowed(english)
            }
        }
        UiLanguage::Russian => {
            if let Some((_, _, _, _, ru)) = DICTIONARY.iter().find(|(key, _, _, _, _)| *key == english) {
                Cow::Borrowed(ru)
            } else {
                Cow::Borrowed(english)
            }
        }
    }
}

/// Consolidated single-source-of-truth dictionary: `(English Key, Chinese (zh), Japanese (ja), Korean (ko), Russian (ru))`
const DICTIONARY: &[(&str, &str, &str, &str, &str)] = &[
    ("Live typing", "实时输入", "リアルタイム入力", "실시간 입력", "Ввод в реальном времени"),
    ("Expires in", "距离清空", "自動消去まで", "자동 삭제까지", "Авточистка через"),
    ("Idle", "空闲", "待機中", "대기 중", "Ожидание"),
    ("Translation", "翻译", "翻訳", "번역", "Перевод"),
    ("Settings", "设置", "設定", "설정", "Настройки"),
    ("VRChat OSC", "VRChat OSC", "VRChat OSC", "VRChat OSC", "VRChat OSC"),
    ("VRChat OSC Studio", "VRChat OSC 工作台", "VRChat OSC スタジオ", "VRChat OSC 스튜디오", "VRChat OSC Студия"),
    ("User Guide", "使用指南", "ユーザーガイド", "사용자 가이드", "Руководство"),
    ("Guide", "指南", "ガイド", "가이드", "Справка"),
    ("Voice Route", "语音语言", "音声言語", "음성 언어", "Язык речи"),
    ("Input:", "输入：", "入力:", "입력:", "Вход:"),
    ("Pair:", "语言对：", "言語対:", "언어 쌍:", "Пара:"),
    ("Auto (bidirectional)", "自动（双向）", "自動（双方向）", "자동 (양방향)", "Авто (двунаправленный)"),
    ("Chinese", "中文", "中国語", "중국어", "Китайский"),
    ("English", "英语", "英語", "영어", "Английский"),
    ("French", "法语", "フランス語", "프랑스어", "Французский"),
    ("Portuguese", "葡萄牙语", "ポルトガル語", "포르투갈어", "Португальский"),
    ("Spanish", "西班牙语", "スペイン語", "스페인어", "Испанский"),
    ("Japanese", "日语", "日本語", "일본어", "Японский"),
    ("Russian", "俄语", "ロシア語", "러시아어", "Русский"),
    ("Korean", "韩语", "韓国語", "한국어", "Корейский"),
    ("Thai", "泰语", "タイ語", "태국어", "Тайский"),
    ("Italian", "意大利语", "イタリア語", "이탈리아어", "Итальянский"),
    ("German", "德语", "ドイツ語", "독일어", "Немецкий"),
    ("Vietnamese", "越南语", "ベトナム語", "베트남어", "Вьетнамский"),
    ("Indonesian", "印度尼西亚语", "インドネシア語", "인도네시아어", "Индонезийский"),
    ("Polish", "波兰语", "ポーランド語", "폴란드어", "Польский"),
    ("Czech", "捷克语", "チェコ語", "체코어", "Чешский"),
    ("Dutch", "荷兰语", "オランダ語", "네덜란드어", "Нидерландский"),
    ("Chinese <-> English", "中文 <-> 英语", "中国語 <-> 英語", "중국어 <-> 영어", "Китайский <-> Английский"),
    ("Japanese <-> English", "日语 <-> 英语", "日本語 <-> 英語", "일본어 <-> 영어", "Японский <-> Английский"),
    ("Japanese <-> Chinese", "日语 <-> 中文", "日本語 <-> 中国語", "일본어 <-> 중국어", "Японский <-> Китайский"),
    ("Unknown language", "未知语言", "不明な言語", "알 수 없는 언어", "Неизвестный язык"),
    ("Audio Input", "音频输入", "音声入力", "오디오 입력", "Аудиовход"),
    ("Source:", "来源：", "入力元:", "소스:", "Источник:"),
    ("Microphone", "麦克风", "マイク", "마이크", "Микрофон"),
    ("System Audio (WASAPI)", "系统音频（WASAPI）", "システム音声（WASAPI）", "시스템 오디오 (WASAPI)", "Системный звук (WASAPI)"),
    ("Refresh", "刷新", "更新", "새로고침", "Обновить"),
    ("Start Translation", "开始翻译", "翻訳開始", "번역 시작", "Начать перевод"),
    ("Stop Translation", "停止翻译", "翻訳停止", "번역 중지", "Остановить перевод"),
    ("Device:", "设备：", "デバイス:", "장치:", "Устройство:"),
    ("Input level:", "输入音量：", "音量レベル:", "입력 레벨:", "Уровень входа:"),
    ("Waiting for audio", "等待音频", "音声待機中", "오디오 대기 중", "Ожидание аудио"),
    ("Default microphone", "默认麦克风", "デフォルトマイク", "기본 마이크", "Микрофон по умолчанию"),
    ("Default render output (loopback)", "默认播放输出（回环）", "デフォルト再生出力（ループバック）", "기본 출력 (루프백)", "Выход по умолчанию (loopback)"),
    ("Recognition History", "识别记录", "音声認識履歴", "인식 기록", "История распознавания"),
    ("Translation History", "翻译记录", "翻訳履歴", "번역 기록", "История перевода"),
    ("No speech recognized yet...", "尚未识别到语音…", "音声がまだ認識されていません...", "아직 인식된 음성이 없습니다...", "Речь пока не распознана..."),
    ("No translations emitted yet...", "尚未产生翻译…", "翻訳がまだ生成されていません...", "아직 생성된 번역이 없습니다...", "Перевод пока не создан..."),
    ("View Detailed Log", "查看详细日志", "詳細ログを表示", "자세한 로그 보기", "Подробный журнал"),
    ("Detailed Error Traceback", "详细错误追踪", "エラー詳細トレース", "자세한 오류 추적", "Подробная трассировка ошибок"),
    ("All Sections", "全部设置", "すべての設定", "모든 항목", "Все разделы"),
    ("General & Appearance", "常规与外观", "一般・外観", "일반 및 UI", "Общие и внешний вид"),
    ("Service Providers", "服务提供商", "プロバイダー", "서비스 제공자", "Провайдеры"),
    ("Audio & Integration", "音频与集成", "音声・連携", "오디오 및 연동", "Аудио и интеграция"),
    ("Translation Server", "翻译服务器", "翻訳サーバー", "번역 서버", "Сервер перевода"),
    ("Local Service", "本地服务", "ローカルサービス", "로컬 서비스", "Локальная служба"),
    ("Application Language", "界面语言", "表示言語", "앱 언어", "Язык интерфейса"),
    ("Native Backend & Models", "原生后端与模型", "ネイティブバックエンド＆モデル", "네이티브 백엔드 및 모델", "Нативный бэкенд и модели"),
    ("Local Service & Models", "本地服务与模型", "ローカルサービス＆モデル", "로컬 서비스 및 모델", "Локальная служба и модели"),
    ("Native Model Packages", "原生模型包", "ネイティブモデルパッケージ", "네이티브 모델 패키지", "Пакеты нативных моделей"),
    (
        "Install packages explicitly. Downloads, resume handling, and SHA-256 verification run in the background; model files are activated only after verification succeeds.",
        "模型包需要显式安装。下载、断点续传和 SHA-256 校验均在后台执行；只有校验成功后才会启用模型文件。",
        "モデルパッケージのインストールが必要です。ダウンロード・再開・SHA-256検証はバックグラウンドで実行され、検証成功後に有効化されます。",
        "모델 패키지를 명시적으로 설치해야 합니다. 다운로드 및 SHA-256 검증이 백그라운드에서 실행되며 성공 후 활성화됩니다.",
        "Установите пакеты моделей. Загрузка и проверка SHA-256 выполняются в фоновом режиме.",
    ),
    (
        "Native packages currently include speech recognition and translation models. Local TTS is unavailable in the native backend; keep tts.provider set to none.",
        "原生模型包目前包含语音识别模型与翻译模型。本地 TTS 尚不能由原生后端运行；请保持 tts.provider 为 none。",
        "ネイティブパッケージには現在、音声認識モデルと翻訳モデルが含まれています。ローカルTTSは未対応のため、tts.providerはnoneのままにしてください。",
        "네이티브 패키지에는 현재 음성 인식 및 번역 모델이 포함됩니다. 로컬 TTS는 미지원이므로 tts.provider를 none으로 유지하세요.",
        "Нативные пакеты включают модели распознавания речи и перевода. Локальный TTS не поддерживается.",
    ),
    ("Install Speech Recognition Model", "安装语音识别模型", "音声認識モデルをインストール", "음성 인식 모델 설치", "Установить модель распознавания речи"),
    ("Install Translation Model", "安装翻译模型", "翻訳モデルをインストール", "번역 모델 설치", "Установить модель перевода"),
    ("Verify Native Models", "校验原生模型", "ネイティブモデルを検証", "네이티브 모델 검증", "Проверить нативные модели"),
    (
        "Native models have not been verified yet.",
        "原生模型尚未校验。",
        "ネイティブモデルはまだ検証されていません。",
        "네이티브 모델이 아직 검증되지 않았습니다.",
        "Нативные модели еще не проверены.",
    ),
    ("Installing native model package:", "正在安装原生模型包：", "モデルパッケージをインストール中:", "네이티브 모델 패키지 설치 중:", "Установка пакета моделей:"),
    (
        "Preparing native model installation…",
        "正在准备原生模型安装…",
        "モデルのインストールを準備中…",
        "네이티브 모델 설치 준비 중…",
        "Подготовка к установке моделей…",
    ),
    (
        "Verifying native model SHA-256 checksums in the background…",
        "正在后台校验原生模型的 SHA-256 校验和…",
        "バックグラウンドでモデルのSHA-256検証中…",
        "백그라운드에서 SHA-256 검증 중…",
        "Проверка контрольных сумм SHA-256…",
    ),
    (
        "Native model package installed and verified.",
        "原生模型包已安装并校验通过。",
        "モデルパッケージのインストールと検証が完了しました。",
        "네이티브 모델 패키지 설치 및 검증 완료.",
        "Пакет моделей установлен и проверен.",
    ),
    (
        "All native model packages passed SHA-256 verification.",
        "所有原生模型包均已通过 SHA-256 校验。",
        "すべてのモデルパッケージがSHA-256検証に合格しました。",
        "모든 네이티브 모델 패키지가 SHA-256 검증을 통과했습니다.",
        "Все пакеты моделей успешно прошли проверку SHA-256.",
    ),
    ("Native model task failed:", "原生模型任务失败：", "ネイティブモデルタスク失敗:", "네이티브 모델 작업 실패:", "Сбой задачи модели:"),
    ("Speech Recognition Model", "语音识别模型", "音声認識モデル", "음성 인식 모델", "Модель распознавания речи"),
    ("Translation Model", "翻译模型", "翻訳モデル", "번역 모델", "Модель перевода"),
    ("llama-server path:", "llama-server 路径：", "llama-server パス:", "llama-server 경로:", "Путь к llama-server:"),
    ("Choose llama-server.exe", "选择 llama-server.exe", "llama-server.exe を選択", "llama-server.exe 선택", "Выбрать llama-server.exe"),
    ("Browse…", "浏览…", "参照…", "찾아보기…", "Обзор…"),
    ("Save llama.cpp Path", "保存 llama.cpp 路径", "llama.cpp パスを保存", "llama.cpp 경로 저장", "Сохранить путь к llama.cpp"),
    (
        "XRTranslate starts the native Rust backend and its local llama.cpp model servers. Leave this empty to use the bundled or workspace backend executable.",
        "XRTranslate 会启动原生 Rust 后端及本地 llama.cpp 模型服务。留空则使用随附或工作区中的后端可执行文件。",
        "XRTranslateはRustバックエンドとローカルllama.cppサーバーを起動します。空欄の場合は付属の実行ファイルを使用します。",
        "XRTranslate는 네이티브 Rust 백엔드 및 llama.cpp 모델 서버를 실행합니다. 비워두면 내장 실행 파일을 사용합니다.",
        "XRTranslate запускает Rust бэкенд и сервера моделей llama.cpp. Оставьте пустым для встроенного файла.",
    ),
    (
        "XRTranslate starts the local service when you begin translating. Leave this empty to use the service included with the app.",
        "开始翻译时，XRTranslate 会启动本地服务。留空则使用应用随附的服务。",
        "翻訳開始時にローカルサービスを起動します。空欄の場合は付属サービスを使用します。",
        "번역 시작 시 로컬 서비스를 실행합니다. 비워두면 앱 내장 서비스를 사용합니다.",
        "XRTranslate запускает службу при начале перевода. Оставьте пустым для встроенного сервиса.",
    ),
    ("Translation Server Endpoint", "翻译服务器端点", "翻訳サーバーエンドポイント", "번역 서버 엔드포인트", "Эндпоинт сервера перевода"),
    ("Server URL:", "服务器地址：", "サーバーURL:", "서버 URL:", "URL сервера:"),
    ("VRChat OSC Network & Rules", "VRChat OSC 网络与规则", "VRChat OSC ネットワーク・ルール", "VRChat OSC 네트워크 및 규칙", "Сеть и правила VRChat OSC"),
    ("Listener Status:", "监听状态：", "リスナー状態:", "리스ナー 상태:", "Статус слушателя:"),
    ("Send Port:", "发送端口：", "送信ポート:", "전송 포트:", "Порт отправки:"),
    ("Listen Port:", "监听端口：", "受信ポート:", "수신 포트:", "Порт прослушивания:"),
    ("Character Limit:", "字符上限：", "文字数制限:", "글자 수制限:", "Лимит символов:"),
    ("History TTL (sec):", "历史保留时间（秒）：", "履歴保持時間（秒）:", "기록 보존 시간(초):", "TTL истории (сек):"),
    ("Apply & Restart Listener", "应用并重启监听器", "適用してリスナーを再起動", "적용 및 리스너 재시작", "Применить и перезапустить"),
    ("Enable TTS Audio Playback", "启用 TTS 音频播放", "TTS音声再生を有効化", "TTS 음성 재생 활성화", "Включить озвучку TTS"),
    (
        "Pause translation when VRChat microphone is muted (/MuteSelf)",
        "VRChat 麦克风静音时暂停翻译（/MuteSelf）",
        "VRChatマイクのミュート時に翻訳を一時停止（/MuteSelf）",
        "VRChat 마이크 음소거 시 번역 일시중지 (/MuteSelf)",
        "Пауза перевода при муте микрофона VRChat (/MuteSelf)",
    ),
    ("Enable OSC", "启用 OSC", "OSCを有効化", "OSC 활성화", "Включить OSC"),
    ("Disabled", "已禁用", "無効", "비활성화됨", "Отключено"),
    ("Muted", "已静音", "ミュート中", "음소거됨", "Заглушено"),
    ("Active", "活动中", "有効", "활성", "Активно"),
    ("Text Format:", "文本格式：", "テキスト形式:", "텍스트 형식:", "Формат текста:"),
    ("Speaker Number", "说话人序号", "話者番号", "화자 번호", "Номер говорящего"),
    ("Clear Chatbox", "清空聊天框", "チャットボックスを消去", "채팅창 지우기", "Очистить чатбокс"),
    ("Header Prefix:", "页眉前缀：", "ヘッダー接頭辞:", "머리글 접두사:", "Префикс заголовка:"),
    ("Footer Suffix:", "页脚后缀：", "フッター接尾辞:", "바닥글 접미사:", "Суффикс футера:"),
    ("None (Disabled)", "无（已禁用）", "なし（無効）", "없음 (비활성화)", "Нет (отключено)"),
    ("Custom Text", "自定义文本", "カスタムテキスト", "사용자 지정 텍스트", "Пользовательский текст"),
    ("System Time", "系统时间", "システム時刻", "시스템 시간", "Системное время"),
    ("CPU Usage", "CPU 使用率", "CPU使用率", "CPU 사용량", "Загрузка ЦП"),
    ("GPU Usage", "GPU 使用率", "GPU使用率", "GPU 사용量", "Загрузка ГП"),
    ("(Off)", "（关闭）", "（オフ）", "(꺼짐)", "(Выкл)"),
    ("Auto-synced with system clock", "自动与系统时钟同步", "システム時計と自動同期", "시스템 시계와 자동 동기화", "Автосинхронизация с часами"),
    ("Full Name", "完整名称", "フルネーム", "전체 이름", "Полное имя"),
    ("Bilingual (Source → Target)", "双语（原文 → 译文）", "二言語（原文 → 訳文）", "이중 언어 (원문 → 번역)", "Двуязычный (исходный → перевод)"),
    ("Bilingual (Target → Source)", "双语（译文 → 原文）", "二言語（訳文 → 原文）", "이중 언어 (번역 → 원문)", "Двуязычный (перевод → исходный)"),
    ("Single Line (Source | Target)", "单行（原文 | 译文）", "1行（原文 | 訳文）", "한 줄 (원문 | 번역)", "Одна строка (исходный | перевод)"),
    ("Target Only", "仅译文", "訳文のみ", "번역만", "Только перевод"),
    ("(Chatbox Cleared / Empty)", "（聊天框已清空／为空）", "（チャットボックス消去済み／空）", "(채팅창이 비워짐 / 비어 있음)", "(Чатбокс очищен / пуст)"),
    ("NAVIGATE", "导航", "ナビゲーション", "탐색", "НАВИГАЦИЯ"),
    ("VRChat Muted", "VRChat 已静音", "VRChat ミュート中", "VRChat 음소거됨", "VRChat заглушен"),
    ("VRChat Active", "VRChat 活动中", "VRChat 有効", "VRChat 활성", "VRChat активен"),
    ("Copy Log", "复制日志", "ログをコピー", "로그 복사", "Копировать журнал"),
    ("Page", "页", "ページ", "페이지", "Стр."),
    ("Step", "步骤", "ステップ", "단계", "Шаг"),
    ("Prev", "上一页", "前へ", "이전", "Назад"),
    ("Next", "下一页", "次へ", "다음", "Далее"),
    ("Close", "关闭", "閉じる", "닫기", "Закрыть"),
    ("Finish", "完成", "完了", "완료", "Готово"),
    ("OK", "确定", "OK", "확인", "ОК"),
    ("About & Open Source", "关于与开源", "情報・オープンソース", "정보 및 오픈 소스", "О программе и Open Source"),
    ("App Version:", "应用程序版本：", "アプリバージョン:", "앱 버전:", "Версия приложения:"),
    ("GitHub Repository:", "GitHub 仓库地址：", "GitHub リポジトリ:", "GitHub 리포지토리:", "Репозиторий GitHub:"),
    ("Service Providers", "服务提供商", "プロバイダー", "서비스 제공자", "Провайдеры"),
    (
        "Choose active providers and configure parameters for config.json. Changes take effect after saving and restarting the backend.",
        "选择服务提供商并配置 config.json 参数。保存并重启后端后生效。",
        "アクティブなプロバイダーを選択し、config.jsonを設定します。変更は保存と再起動後に反映されます。",
        "활성 제공자를 선택하고 config.json을 설정합니다. 저장 및 백엔드 재시작 후 적용됩니다.",
        "Выберите провайдеров и настройте config.json. Изменения вступят в силу после перезапуска.",
    ),
    ("ASR / Speech Recognition", "ASR／语音识别", "ASR / 音声認識", "ASR / 음성 인식", "ASR / Распознавание речи"),
    ("Active Provider:", "当前服务：", "使用中プロバイダー:", "활성 제공자:", "Активный провайдер:"),
    ("No providers configured", "未配置服务提供商", "プロバイダー未設定", "설정된 제공자 없음", "Провайдеры не настроены"),
    ("Show All Providers", "显示所有服务商", "すべてのプロバイダーを表示", "모든 제공자 보기", "Показать всех провайдеров"),
    (
        "No providers found in config.json.",
        "config.json 中未找到服务提供商。",
        "config.json にプロバイダーが見つかりません。",
        "config.json에서 제공자를 찾을 수 없습니다.",
        "Провайдеры не найдены в config.json.",
    ),
    ("(Active)", "（当前）", "（アクティブ）", "(활성)", "(Активен)"),
    ("Check model files", "检查模型文件", "モデルファイルを確認", "모델 파일 확인", "Проверить файлы моделей"),
    ("Download", "下载", "ダウンロード", "다운로드", "Скачать"),
    ("Verify", "校验", "検証", "검증", "Проверить"),
    ("Verified", "已校验", "検証済み", "검증됨", "Проверено"),
    ("Model package verified.", "模型包已通过校验。", "モデルパッケージ検証完了。", "모델 패키지 검증 완료.", "Пакет моделей проверен."),
    (
        "Model files found. Verify before use.",
        "已找到模型文件，请先校验后使用。",
        "モデルファイルが見つかりました。使用前に検証してください。",
        "모델 파일을 찾았습니다. 사용 전 검증하세요.",
        "Файлы найдены. Проверьте перед использованием.",
    ),
    (
        "Preparing native model installation...",
        "正在准备模型安装...",
        "モデルのインストールを準備中...",
        "모델 설치 준비 중...",
        "Подготовка установки моделей...",
    ),
    (
        "Choose an existing llama-server.exe to continue.",
        "请选择已有的 llama-server.exe 后再继续。",
        "既存の llama-server.exe を選択して続行してください。",
        "기존 llama-server.exe를 선택하세요.",
        "Выберите существующий llama-server.exe.",
    ),
    (
        "Install every required model package to continue.",
        "请安装所有必需的模型包后再继续。",
        "必要なモデルパッケージをすべてインストールして続行してください。",
        "필수 모델 패키지를 모두 설치하세요.",
        "Установите все необходимые пакеты моделей.",
    ),
    (
        "Wait for the current model task to finish.",
        "请等待当前模型任务完成。",
        "現在のモデルタスクの完了をお待ちください。",
        "현재 모델 작업이 완료될 때까지 기다리세요.",
        "Дождитесь завершения текущей задачи.",
    ),
    ("Install automatically (recommended)", "自动安装（推荐）", "自動インストール（推奨）", "자동 설치 (권장)", "Автоустановка (рекомендуется)"),
    (
        "Detects your hardware automatically and downloads the optimal package.",
        "自动检测电脑配置并一键下载匹配的加速包。",
        "ハードウェアを自動検出して最適なパッケージをダウンロードします。",
        "하드웨어를 자동 감지하여 최적의 패키지를 다운로드합니다.",
        "Автоопределение железа и скачивание оптимального пакета.",
    ),
    (
        "XRTranslate checks your NVIDIA GPU, CUDA driver support, and compute capability. It chooses the matching official CUDA package, or the CPU package when CUDA is unavailable.",
        "自动检测电脑配置并一键下载匹配的加速包。",
        "NVIDIA GPUとCUDAを検出し、最適な公式パッケージを自動選択します。",
        "NVIDIA GPU 및 CUDA를 확인하여 최적의 공식 패키지를 자동 선택합니다.",
        "Проверяет GPU NVIDIA и CUDA, выбирая подходящий официальный пакет.",
    ),
    ("Download and install llama.cpp", "下载并安装 llama.cpp", "llama.cpp をダウンロード＆インストール", "llama.cpp 다운로드 및 설치", "Скачать и установить llama.cpp"),
    (
        "Detecting the recommended runtime...",
        "正在检测推荐的运行时...",
        "推奨ランタイムを検出中...",
        "권장 런타임 감지 중...",
        "Определение рекомендуемой среды…",
    ),
    ("Extracting llama.cpp...", "正在解压 llama.cpp...", "llama.cpp を解凍中...", "llama.cpp 압축 해제 중...", "Распаковка llama.cpp…"),
    (
        "llama.cpp is installed and ready.",
        "llama.cpp 已安装并可使用。",
        "llama.cpp のインストールが完了しました。",
        "llama.cpp 설치 완료.",
        "llama.cpp установлен и готов.",
    ),
    ("No configurable parameters", "没有可配置参数", "設定可能なパラメーターはありません", "설정 가능한 매개변수 없음", "Нет параметров для настройки"),
    (
        "No configurable parameters for this provider.",
        "该服务商没有可配置参数。",
        "このプロバイダーに設定可能なパラメーターはありません。",
        "이 제공자에는 설정 가능한 매개변수가 없습니다.",
        "У этого провайдера нет параметров."),
    ("Save Service Config *", "保存服务配置 *", "サービス設定を保存 *", "서비스 설정 저장 *", "Сохранить конфиг *"),
    ("Save Service Config", "保存服务配置", "サービス設定を保存", "서비스 설정 저장", "Сохранить конфиг"),
    ("Reload", "重新加载", "再読み込み", "새로고침", "Перезагрузить"),
    ("(Unsaved changes)", "（有未保存的更改）", "（未保存の変更あり）", "(저장되지 않은 변경 사항)", "(Несохраненные изменения)"),
    ("Text", "文本", "テキスト", "텍스트", "Текст"),
    ("Number", "数字", "数値", "숫자", "Число"),
    ("JSON value", "JSON 值", "JSON値", "JSON 값", "Значение JSON"),
    ("chars", "个字符", "文字", "자", "симв."),
    (
        "XRTranslate Welcome & Onboarding",
        "XRTranslate 欢迎与引导",
        "XRTranslate ウェルカムガイド",
        "XRTranslate 환영 및 가이드",
        "Добро пожаловать в XRTranslate",
    ),
    ("Section", "章节", "セクション", "섹션", "Раздел"),
    ("Overview", "概览", "概要", "개요", "Обзор"),
    ("Quickstart", "快速开始", "クイックスタート", "빠른 시작", "Быстрый старт"),
    ("Welcome", "欢迎", "ようこそ", "환영합니다", "Добро пожаловать"),
    ("Get llama.cpp", "获取 llama.cpp", "llama.cpp を入手", "llama.cpp 받기", "Скачать llama.cpp"),
    ("Install models", "安装模型", "モデルをインストール", "모델 설치", "Установить модели"),
    ("Start translating", "开始翻译", "翻訳を開始", "번역 시작", "Начать перевод"),
    (
        "A calm start, one step at a time",
        "从容开始，一步一步完成设置",
        "ステップバイステップでスムーズにセットアップ",
        "단계별로 여유롭게 시작하세요",
        "Спокойный старт, шаг за шагом",
    ),
    ("Back", "上一步", "戻る", "이전", "Назад"),
    ("Continue", "继续", "次へ", "계속", "Продолжить"),
    ("Finish later", "稍后完成", "後で完了", "나중에 완료", "Завершить позже"),
    ("Open Translation", "打开翻译", "翻訳画面を開く", "번역 열기", "Открыть перевод"),
    ("VRChat subtitles", "VRChat 字幕", "VRChat 字幕", "VRChat 자막", "Субтитры VRChat"),
    ("Get started", "开始使用", "始めましょう", "시작하기", "Начать"),
    ("Welcome to XRTranslate", "欢迎使用 XRTranslate", "XRTranslate へようこそ", "XRTranslate에 오신 것을 환영합니다", "Добро пожаловать в XRTranslate"),
    (
        "Set up your local tools once, then keep conversations flowing naturally.",
        "只需完成一次准备，之后便可自然地跟上每一段对话。",
        "一度セットアップすれば、会話を自然に楽しむことができます。",
        "한 번 설정하면 대화를 자연스럽게 이어나갈 수 있습니다.",
        "Настройте один раз и общайтесь свободно.",
    ),
    ("Bring your own voice", "从声音开始", "音声を入力", "음성 입력", "Ваш голос"),
    (
        "Use a microphone or the sound already playing on your computer.",
        "可以使用麦克风，也可以使用电脑正在播放的声音。",
        "マイクやPCで再生中の音声を使用できます。",
        "마이크 또는 컴퓨터에서 재생 중인 오디오를 사용하세요.",
        "Используйте микрофон или системный звук PC.",
    ),
    ("Understand together", "轻松理解", "直感的に理解", "함께 이해하기", "Понимайте вместе"),
    (
        "Read the original words and translation side by side.",
        "原文与译文并排呈现，阅读更自然。",
        "原文と訳文を並べて表示し、スムーズに読めます。",
        "원문과 번역문을 나란히 편하게 읽으세요.",
        "Читайте оригинал и перевод бок о бок.",
    ),
    ("Share when ready", "随时分享", "いつでも共有", "언제든 공유", "Делитесь когда готовы"),
    (
        "Send subtitles to VRChat whenever you choose.",
        "需要时，可将字幕发送到 VRChat。",
        "必要な時に字幕を VRChat へ送信できます。",
        "원할 때 언제든 VRChat으로 자막을 전송하세요.",
        "Отправляйте субтитры в VRChat в любой момент.",
    ),
    ("Download llama.cpp", "下载 llama.cpp", "llama.cpp をダウンロード", "llama.cpp 다운로드", "Скачать llama.cpp"),
    (
        "Choose the small local program that will run your models.",
        "选择用于运行模型的本地程序。",
        "モデルを実行するローカルプログラムを選択します。",
        "모델을 실행할 로컬 프로그램을 선택하세요.",
        "Выберите программу для запуска моделей.",
    ),
    (
        "Select automatic one-click setup or specify an existing build.",
        "可以选择一键自动下载配置，或指定已有版本。",
        "ワンクリック自動設定、または既存ビルドを指定できます。",
        "원클릭 자동 설정 또는 기존 빌드를 지정하세요.",
        "Выберите автоустановку или укажите свой файл.",
    ),
    (
        "Download one local runtime, keep its files together, then choose llama-server.exe.",
        "可以选择一键自动下载配置，或指定已有版本。",
        "ランタイムをダウンロードし、解凍フォルダ内の llama-server.exe を選択します。",
        "런타임을 다운로드하고 폴더 내 llama-server.exe를 선택하세요.",
        "Скачайте архив и выберите llama-server.exe.",
    ),
    (
        "Option A: Automatic Setup (Recommended)",
        "方案 A：一键自动配置（推荐）",
        "オプション A: 自動設定（推奨）",
        "옵션 A: 자동 설정 (권장)",
        "Вариант A: Автонастройка (Рекомендуется)",
    ),
    (
        "Option B: Install Manually",
        "方案 B：手动下载与安装（备选）",
        "オプション B: 手動インストール（代替）",
        "옵션 B: 수동 설치 (대안)",
        "Вариант B: Ручная установка",
    ),
    (
        "If automatic download fails or you prefer using an existing llama.cpp build:",
        "如果自动下载无法使用，或希望复用已有的 llama.cpp 版本：",
        "自動ダウンロードが失敗した場合、または既存の llama.cpp を使用する場合:",
        "자동 다운로드가 실패하거나 기존 빌드를 사용하는 경우:",
        "Если автоскачивание не работает или у вас есть свой llama.cpp:",
    ),
    (
        "1. Download the right package manually",
        "1. 手动下载正确的安装包",
        "1. 適切なパッケージを手動ダウンロード",
        "1. 올바른 패키지 수동 다운로드",
        "1. Скачайте нужный пакет вручную",
    ),
    ("1. Download the right package", "1. 下载正确的安装包", "1. 適切なパッケージをダウンロード", "1. 올바른 패키지 다운로드", "1. Скачайте нужный пакет"),
    (
        "NVIDIA GPU: Download matching CUDA package and CUDA runtime DLLs.",
        "NVIDIA 显卡：下载匹配的 CUDA 安装包与运行库 DLL。",
        "NVIDIA GPU: 対応する CUDA パッケージと DLL をダウンロードしてください。",
        "NVIDIA GPU: CUDA 패키지 및 DLL을 다운로드하세요.",
        "NVIDIA GPU: Скачайте пакет CUDA и DLL среды.",
    ),
    (
        "NVIDIA graphics: download the matching llama-...-bin-win-cuda-...-x64.zip and cudart-llama-bin-win-cuda-...-x64.zip from the same release and CUDA version.",
        "NVIDIA 显卡：从同一个发布版本下载 CUDA 版本一致的 llama-...-bin-win-cuda-...-x64.zip 与 cudart-llama-bin-win-cuda-...-x64.zip。",
        "NVIDIA GPU: リリース一覧から同じバージョンの CUDA 用 zip と DLL をダウンロードしてください。",
        "NVIDIA GPU: 동일 버전의 CUDA zip 파일 및 DLL을 다운로드하세요.",
        "NVIDIA: Скачайте совместимые архивы llama-bin и cudart-llama.",
    ),
    (
        "CPU only: download llama-...-bin-win-cpu-x64.zip.",
        "仅使用 CPU：下载 llama-...-bin-win-cpu-x64.zip。",
        "CPUのみ: llama-...-bin-win-cpu-x64.zip をダウンロードしてください。",
        "CPU 전용: llama-...-bin-win-cpu-x64.zip을 다운로드하세요.",
        "Только CPU: Скачайте llama-...-bin-win-cpu-x64.zip.",
    ),
    ("2. Keep the runtime together", "2. 将运行时文件放在一起", "2. ランタイムファイルを同じフォルダに保存", "2. 런타임 파일 함께 보관", "2. Сохраните файлы в одной папке"),
    (
        "Extract every file into one folder, for example D:\\llama.cpp. With NVIDIA, llama-server.exe and cudart64_*.dll must stay in that same folder.",
        "将所有文件解压到同一个文件夹，例如 D:\\llama.cpp。使用 NVIDIA 时，llama-server.exe 与 cudart64_*.dll 必须位于同一文件夹。",
        "すべてのファイルを同じフォルダ（例: D:\\llama.cpp）に解凍してください。NVIDIAの場合、llama-server.exe と cudart64_*.dll は同じフォルダに配置する必要があります。",
        "모든 파일을 한 폴더(예: D:\\llama.cpp)에 압축 해제하세요. NVIDIA의 경우 llama-server.exe와 DLL이 같은 폴더에 있어야 합니다.",
        "Распакуйте файлы в одну папку (напр. D:\\llama.cpp). Для NVIDIA файлы llama-server.exe и cudart64_*.dll должны быть вместе.",
    ),
    (
        "Choose the build that fits your computer",
        "选择适合电脑的版本",
        "PCに合ったビルドを選択",
        "PC에 맞는 빌드 선택",
        "Выберите сборку для вашего PC",
    ),
    (
        "For NVIDIA, download the matching llama.cpp and CUDA runtime packages. For CPU use, choose the CPU package. Keep the extracted files together.",
        "使用 NVIDIA 显卡时，请下载版本匹配的 llama.cpp 与 CUDA 运行库；仅使用 CPU 时请选择 CPU 包。解压后的文件请放在同一个文件夹中。",
        "NVIDIAの場合は対応する llama.cpp と CUDA パッケージをダウンロードし、解凍ファイルを同じフォルダに保存してください。",
        "NVIDIA의 경우 CUDA 패키지를, CPU의 경우 CPU 패키지를 다운로드하고 한 폴더에 보관하세요.",
        "Для NVIDIA скачайте пакет CUDA, для CPU — пакет CPU. Держите распакованные файлы в одной папке.",
    ),
    ("Open llama.cpp downloads", "打开 llama.cpp 下载页", "llama.cpp ダウンロードページを開く", "llama.cpp 다운로드 페이지 열기", "Открыть загрузки llama.cpp"),
    ("Select llama-server.exe", "选择 llama-server.exe", "llama-server.exe を選択", "llama-server.exe 선택", "Выбрать llama-server.exe"),
    ("Save path", "保存路径", "パスを保存", "경로 저장", "Сохранить путь"),
    (
        "Please choose llama-server.exe first.",
        "请先选择 llama-server.exe。",
        "まず llama-server.exe を選択してください。",
        "먼저 llama-server.exe를 선택하세요.",
        "Сначала выберите llama-server.exe.",
    ),
    ("Install your model packages", "安装模型包", "モデルパッケージをインストール", "모델 패키지 설치", "Установить пакеты моделей"),
    (
        "Download the two recommended packages. This may take a little while.",
        "下载两个推荐模型包。这个过程可能需要一点时间。",
        "推奨の2つのモデルパッケージをダウンロードします。これには少し時間がかかる場合があります。",
        "권장하는 두 패키지를 다운로드합니다. 시간이 조금 걸릴 수 있습니다.",
        "Загрузите два рекомендуемых пакета. Это может занять некоторое время.",
    ),
    ("Qwen3-ASR", "语音识别", "音声認識", "음성 인식", "Распознавание речи"),
    (
        "Listens to speech and turns it into text.",
        "将语音转换为文字。",
        "声を聴き取ってテキストに変換します。",
        "음성을 듣고 텍스트로 변환합니다.",
        "Распознает речь и преобразует ее в текст.",
    ),
    ("Hy-MT2", "翻译", "翻訳", "번역", "Перевод"),
    ("Translates the text for you.", "将文字翻译成目标语言。", "テキストを目的の言語に翻訳します。", "텍스트를 원하는 언어로 번역합니다.", "Переводит текст на выбранный язык."),
    ("Installed", "已安装", "インストール済み", "설치됨", "Установлено"),
    ("Install", "安装", "インストール", "설치", "Установить"),
    ("Turns speech into text.", "将语音转换为文字。", "声をテキストに変換します。", "음성을 텍스트로 변환합니다.", "Преобразует речь в текст."),
    ("Translates text for you.", "为你翻译文字。", "テキストを翻訳します。", "텍스트를 번역해 드립니다.", "Переводит текст для вас."),
    ("Verify models", "校验模型", "モデルを検証", "모델 검증", "Проверить модели"),
    (
        "Install both packages, then verify them here.",
        "安装两个模型包后，再在这里进行校验。",
        "両方のパッケージをインストール後、ここで検証してください。",
        "두 패키지를 설치한 후 여기서 검증하세요.",
        "Установите оба пакета и проверьте их здесь.",
    ),
    (
        "Looking for existing model packages...",
        "正在查找已有模型包...",
        "既存のモデルパッケージを検索中...",
        "기존 모델 패키지 검색 중...",
        "Поиск существующих моделей…",
    ),
    (
        "One model package is ready. Install the remaining package.",
        "一个模型包已准备好。请安装另一个模型包。",
        "1つのモデルパッケージの準備が完了しました。残りのパッケージをインストールしてください。",
        "하나의 모델 패키지가 준비되었습니다. 남은 패키지를 설치하세요.",
        "Один пакет готов. Установите оставшийся.",
    ),
    (
        "Choose a model package to install.",
        "请选择要安装的模型包。",
        "インストールするモデルパッケージを選択してください。",
        "설치할 모델 패키지를 선택하세요.",
        "Выберите пакет моделей для установки.",
    ),
    ("Your model packages are installed.", "模型包已安装。", "モデルパッケージがインストールされました。", "모델 패키지가 설치되었습니다.", "Пакеты моделей установлены."),
    (
        "One model package is installed. Install the remaining package.",
        "一个模型包已安装。请安装另一个模型包。",
        "1つのモデルパッケージがインストールされました。残りのパッケージをインストールしてください。",
        "하나의 모델 패키지가 설치되었습니다. 남은 패키지를 설치하세요.",
        "Один пакет установлен. Установите оставшийся.",
    ),
    ("Downloading your model package…", "正在下载模型包…", "モデルパッケージをダウンロード中…", "모델 패키지 다운로드 중…", "Загрузка пакета моделей…"),
    ("Checking your model files…", "正在检查模型文件…", "モデルファイルをチェック中…", "모델 파일 확인 중…", "Проверка файлов моделей…"),
    ("A model package is ready.", "一个模型包已准备好。", "モデルパッケージの準備ができました。", "모델 패키지가 준비되었습니다.", "Пакет моделей готов."),
    ("Your model packages are ready.", "模型包已经准备好。", "モデルパッケージの準備が完了しました。", "모든 모델 패키지가 준비되었습니다.", "Пакеты моделей готовы."),
    ("You are ready to begin", "准备就绪", "準備完了", "시작할 준비가 되었습니다", "Все готово к началу"),
    (
        "Choose an audio source and let the conversation unfold.",
        "选择音频来源，便可以开始跟上每一段对话。",
        "音声ソースを選択して会話を始めましょう。",
        "오디오 소스를 선택하고 대화를 시작하세요.",
        "Выберите источник аудио и начните общение.",
    ),
    (
        "You can return to Settings at any time to change your models or VRChat subtitles.",
        "你随时都可以回到设置，调整模型或 VRChat 字幕。",
        "いつでも設定に戻ってモデルや VRChat 字幕を変更できます。",
        "언제든 설정으로 돌아와 모델이나 자막을 변경할 수 있습니다.",
        "Вы можете вернуться в Настройки в любой момент.",
    ),
    (
        "Choose an audio source, start translation, and show subtitles in VRChat whenever you need them.",
        "选择音频来源，开始翻译，并在需要时将字幕显示到 VRChat 中。",
        "音声ソースを選択し、翻訳を開始し、必要な時に VRChat に字幕を表示します。",
        "오디오 소스를 선택하고 번역을 시작한 후 VRChat에 자막을 표시하세요.",
        "Выберите источник, начните перевод и выводите субтитры в VRChat.",
    ),
    (
        "Enable VRChat subtitles in Settings, then choose the style that feels right for you.",
        "在设置中启用 VRChat 字幕，再选择你喜欢的显示方式。",
        "設定で VRChat 字幕を有効にし、お好みのスタイルを選択してください。",
        "설정에서 VRChat 자막을 활성화하고 원하는 스타일을 선택하세요.",
        "Включите субтитры VRChat в Настройках и выберите стиль.",
    ),
    ("VRChat features", "VRChat 功能", "VRChat 機能", "VRChat 기능", "Функции VRChat"),
    ("Get ready", "准备就绪", "準備完了", "준비 완료", "Подготовка"),
    (
        "In Settings, choose llama-server.exe and install the two recommended model packages. Then return to Translation and begin.",
        "在设置中选择 llama-server.exe，并安装两个推荐的模型包；然后回到翻译页面开始使用。",
        "設定で llama-server.exe を選択し、推奨モデルをインストールしてから翻訳ページに戻ってください。",
        "설정에서 llama-server.exe를 선택하고 모델을 설치한 후 번역 페이지로 돌아오세요.",
        "В Настройках выберите llama-server.exe, установите модели и вернитесь к Переводу.",
    ),
    ("First-time setup", "首次设置", "初回セットアップ", "최초 설정", "Первоначальная настройка"),
    (
        "Get Started & Open XRTranslate",
        "开始使用 XRTranslate",
        "開始して XRTranslate を開く",
        "XRTranslate 시작하기",
        "Начать и открыть XRTranslate",
    ),
    ("Skip Onboarding", "跳过引导", "ガイドをスキップ", "가이드 건너뛰기", "Пропустить вводный курс"),
    ("Ready", "就绪", "準備完了", "준비됨", "Готов"),
    ("Starting local backend...", "正在启动本地后端…", "ローカルバックエンドを起動中…", "로컬 백엔드 시작 중…", "Запуск локального бэкенда…"),
    ("Connecting...", "正在连接…", "接続中…", "연결 중…", "Подключение…"),
    ("Connected - listening", "已连接 - 正在监听", "接続済み - 受信中", "연결됨 - 수신 중", "Подключено - прослушивание"),
    ("Stopped", "已停止", "停止", "중지됨", "Остановлено"),
    ("Connection error", "连接错误", "接続エラー", "연결 오류", "Ошибка подключения"),
];
