#include "editor_adapter.h"

#include <QElapsedTimer>
#include <QSignalSpy>
#include <QTextCursor>
#include <QTextDocument>
#include <QTextCharFormat>
#include <QTest>
#include <algorithm>

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
    QVERIFY(loadMilliseconds < 1'000);
    QVERIFY(formattingMicroseconds < 50'000);
  }

  void typingDirtyTrackingAndTwoPaneSnapshot()
  {
    QString source;
    source.reserve(1'750'000);
    for (int word = 0; word < 250'000; ++word) {
      source += QStringLiteral("harbor ");
      if (word % 40 == 39)
        source += QLatin1Char('\n');
    }
    QTextDocument primary;
    QTextDocument secondary;
    QElapsedTimer timer;
    timer.start();
    primary.setPlainText(source);
    secondary.setPlainText(source.left(source.size() / 4));
    const auto twoPaneLoadMs = timer.elapsed();
    EditorAdapter adapter;
    adapter.setDocumentForTesting(&primary);
    adapter.setFocused(true);
    QSignalSpy dirty(&adapter, &EditorAdapter::incrementalDirty);
    std::vector<qint64> samples;
    samples.reserve(500);
    QTextCursor cursor(&primary);
    cursor.setPosition(primary.characterCount() / 2);
    for (int index = 0; index < 500; ++index) {
      timer.restart();
      cursor.insertText(QStringLiteral("x"));
      QCoreApplication::processEvents();
      samples.push_back(timer.nsecsElapsed() / 1'000);
    }
    std::ranges::sort(samples);
    const auto p95 = samples[samples.size() * 95 / 100];
    const auto p99 = samples[samples.size() * 99 / 100];
    timer.restart();
    const auto snapshot = adapter.semanticBlocks();
    const auto snapshotMs = timer.elapsed();
    qInfo("two_pane_load_ms=%lld typing_p95_us=%lld typing_p99_us=%lld snapshot_ms=%lld revisions=%llu dirty_signals=%lld",
          twoPaneLoadMs,
          p95,
          p99,
          snapshotMs,
          adapter.revision(),
          dirty.size());
    QVERIFY(twoPaneLoadMs < 1'000);
    QVERIFY(p95 < 16'000);
    QVERIFY(p99 < 50'000);
    QVERIFY(!snapshot.isEmpty());
    QVERIFY(adapter.revision() >= 500);
    QCOMPARE(dirty.size(), 500);
  }
};

QTEST_MAIN(EditorBenchmark)
#include "tst_editor_benchmark.moc"
