#include <QElapsedTimer>
#include <QTextCursor>
#include <QTextDocument>
#include <QTextCharFormat>
#include <QTest>

class EditorBenchmark final : public QObject
{
  Q_OBJECT

private slots:
  void quarterMillionWordDocument()
  {
    QString source;
    source.reserve(1'750'000);
    for (int word = 0; word < 250'000; ++word) {
      source += QStringLiteral("orchard ");
      if (word % 25 == 24)
        source += QLatin1Char('\n');
    }

    QTextDocument document;
    QElapsedTimer timer;
    timer.start();
    document.setPlainText(source);
    const auto loadMilliseconds = timer.restart();

    QTextCursor cursor(&document);
    cursor.setPosition(document.characterCount() / 2);
    cursor.insertText(QStringLiteral("x"));
    const auto keystrokeMicroseconds = timer.nsecsElapsed() / 1'000;

    timer.restart();
    cursor.setPosition(0);
    cursor.setPosition(2'000, QTextCursor::KeepAnchor);
    QTextCharFormat bold;
    bold.setFontWeight(QFont::Bold);
    cursor.mergeCharFormat(bold);
    const auto formattingMicroseconds = timer.nsecsElapsed() / 1'000;

    qInfo("editor words=250000 load_ms=%lld keystroke_us=%lld format_us=%lld blocks=%d",
          loadMilliseconds, keystrokeMicroseconds, formattingMicroseconds, document.blockCount());
    QVERIFY(document.characterCount() > 1'000'000);
  }
};

QTEST_MAIN(EditorBenchmark)
#include "tst_editor_benchmark.moc"
