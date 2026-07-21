#include "editor_adapter.h"

#include <QCoreApplication>
#include <QInputMethodEvent>
#include <QQmlComponent>
#include <QQmlEngine>
#include <QScopedPointer>
#include <QSignalSpy>
#include <QTest>
#include <QTextBoundaryFinder>
#include <QTextDocumentFragment>
#include <QTextImageFormat>
#include <QTextList>
#include <QTextDocument>

class EditorAdapterTest final : public QObject
{
  Q_OBJECT

private slots:
  void semanticLoadAndSnapshotKeepExplicitObjectsAndStyles()
  {
    QTextDocument document;
    EditorAdapter adapter;
    adapter.setDocumentForTesting(&document);
    QVariantMap heading;
    heading.insert(QStringLiteral("type"), QStringLiteral("heading"));
    heading.insert(QStringLiteral("level"), 2);
    heading.insert(QStringLiteral("text"), QStringLiteral("Chapter"));
    heading.insert(QStringLiteral("styleId"), QStringLiteral("stable-paragraph-style"));
    QVariantMap pageBreak;
    pageBreak.insert(QStringLiteral("type"), QStringLiteral("page_break"));
    QVariantMap opaque;
    opaque.insert(QStringLiteral("type"), QStringLiteral("opaque"));
    opaque.insert(QStringLiteral("source"), QStringLiteral("@future[exact]"));
    opaque.insert(QStringLiteral("reason"), QStringLiteral("Unsupported extension"));

    adapter.loadSemanticBlocks({ heading, pageBreak, opaque });
    QCOMPARE(adapter.revision(), 0);
    const auto snapshot = adapter.semanticBlocks();
    QCOMPARE(snapshot.size(), 3);
    QCOMPARE(snapshot[0].toMap().value(QStringLiteral("level")).toInt(), 2);
    QCOMPARE(snapshot[0].toMap().value(QStringLiteral("styleId")).toString(),
             QStringLiteral("stable-paragraph-style"));
    QCOMPARE(snapshot[1].toMap().value(QStringLiteral("type")).toString(),
             QStringLiteral("page_break"));
    QCOMPARE(snapshot[2].toMap().value(QStringLiteral("source")).toString(),
             QStringLiteral("@future[exact]"));
    QVERIFY(!document.isUndoAvailable());
  }

  void incrementalDirtyIsRevisionedAndFocusLossRequestsFlush()
  {
    QTextDocument document;
    document.setPlainText(QStringLiteral("one\ntwo"));
    EditorAdapter adapter;
    adapter.setDocumentForTesting(&document);
    QSignalSpy dirty(&adapter, &EditorAdapter::incrementalDirty);
    QSignalSpy flush(&adapter, &EditorAdapter::focusLostFlushRequested);
    adapter.setFocused(true);
    adapter.setCursorPosition(3);
    adapter.insertSceneBreak();
    QVERIFY(adapter.revision() > 0);
    QVERIFY(!dirty.isEmpty());
    const auto arguments = dirty.last();
    QCOMPARE(arguments[0].toULongLong(), adapter.revision());
    QVERIFY(arguments[5].toInt() > arguments[4].toInt());
    adapter.setFocused(false);
    QCOMPARE(flush.size(), 1);
    QCOMPARE(flush[0][0].toULongLong(), adapter.revision());
  }

  void mixedFormattingAndDirectFormattingAreDistinctFromStyle()
  {
    QTextDocument document;
    QTextCursor writer(&document);
    QTextCharFormat bold;
    bold.setFontWeight(QFont::Bold);
    writer.insertText(QStringLiteral("bold"), bold);
    QTextCharFormat plain;
    plain.setFontWeight(QFont::Normal);
    writer.insertText(QStringLiteral("plain"), plain);
    EditorAdapter adapter;
    adapter.setDocumentForTesting(&document);
    adapter.setSelectionStart(0);
    adapter.setSelectionEnd(9);
    QCOMPARE(adapter.boldState(), -1);
    adapter.setCharacterStyle(QStringLiteral("stable-character-style"));
    adapter.clearDirectFormatting();
    QTextCursor probe(&document);
    probe.setPosition(0);
    probe.setPosition(9, QTextCursor::KeepAnchor);
    QCOMPARE(probe.charFormat().property(QTextFormat::UserProperty + 10).toString(),
             QStringLiteral("stable-character-style"));
    QVERIFY(probe.charFormat().fontWeight() < QFont::Bold);
  }

  void computedStylePreviewAndNextStyleRemainSemantic()
  {
    QTextDocument document;
    document.setPlainText(QStringLiteral("Heading"));
    EditorAdapter adapter;
    adapter.setDocumentForTesting(&document);
    adapter.setCursorPosition(7);
    adapter.setSelectionStart(7);
    adapter.setSelectionEnd(7);
    adapter.defineStyle(QStringLiteral("body"),
                        { { QStringLiteral("font-size"), 12.0 } },
                        true,
                        QStringLiteral("body"));
    adapter.defineStyle(QStringLiteral("heading"),
                        { { QStringLiteral("font-weight"), static_cast<int>(QFont::Bold) },
                          { QStringLiteral("alignment"), QStringLiteral("center") } },
                        true,
                        QStringLiteral("body"));
    adapter.setParagraphStyle(QStringLiteral("heading"));
    QCOMPARE(document.begin().blockFormat().property(QTextFormat::UserProperty + 10).toString(),
             QStringLiteral("heading"));
    QCOMPARE(document.begin().blockFormat().alignment(), Qt::AlignHCenter);
    adapter.insertParagraphBreak();
    QCOMPARE(document.lastBlock()
               .blockFormat()
               .property(QTextFormat::UserProperty + 10)
               .toString(),
             QStringLiteral("body"));
  }

  void documentsKeepIndependentUndoState()
  {
    QTextDocument firstDocument;
    QTextDocument secondDocument;
    firstDocument.setPlainText(QStringLiteral("first"));
    secondDocument.setPlainText(QStringLiteral("second"));

    EditorAdapter first;
    EditorAdapter second;
    first.setDocumentForTesting(&firstDocument);
    second.setDocumentForTesting(&secondDocument);
    first.setCursorPosition(5);
    first.insertSceneBreak();

    QVERIFY(firstDocument.isUndoAvailable());
    QVERIFY(!secondDocument.isUndoAvailable());
    QCOMPARE(secondDocument.toPlainText(), QStringLiteral("second"));
  }

  void formattingUsesCursorSemantics()
  {
    QTextDocument document;
    document.setPlainText(QStringLiteral("styled"));
    EditorAdapter adapter;
    adapter.setDocumentForTesting(&document);
    adapter.setSelectionStart(0);
    adapter.setSelectionEnd(6);
    adapter.toggleBold();

    QTextCursor cursor(&document);
    cursor.setPosition(0);
    cursor.setPosition(6, QTextCursor::KeepAnchor);
    QVERIFY(cursor.charFormat().fontWeight() >= QFont::Bold);
  }

  void pageBreakIsSemanticCustomObject()
  {
    QTextDocument document;
    EditorAdapter adapter;
    adapter.setDocumentForTesting(&document);
    adapter.insertPageBreak();

    QTextCursor cursor(&document);
    cursor.movePosition(QTextCursor::NextCharacter, QTextCursor::KeepAnchor);
    QVERIFY(cursor.charFormat().objectType() >= QTextFormat::UserObject);
  }

  void representativeSemanticFormatsRemainExplicit()
  {
    constexpr int stableStyleProperty = QTextFormat::UserProperty + 10;
    constexpr int opaqueSourceProperty = QTextFormat::UserProperty + 11;
    QTextDocument document;
    QTextCursor cursor(&document);

    QTextBlockFormat heading;
    heading.setHeadingLevel(1);
    heading.setProperty(stableStyleProperty, QStringLiteral("style-heading-uuid"));
    heading.setAlignment(Qt::AlignHCenter);
    cursor.setBlockFormat(heading);
    cursor.insertText(QStringLiteral("Heading"));
    cursor.insertBlock();

    QTextCharFormat emphasis;
    emphasis.setFontWeight(QFont::Bold);
    emphasis.setFontItalic(true);
    emphasis.setVerticalAlignment(QTextCharFormat::AlignSuperScript);
    emphasis.setProperty(stableStyleProperty, QStringLiteral("style-character-uuid"));
    cursor.insertText(QStringLiteral("styled"), emphasis);

    QTextCharFormat link;
    link.setAnchor(true);
    link.setAnchorHref(QStringLiteral("https://example.invalid"));
    cursor.insertText(QStringLiteral(" link"), link);
    cursor.insertBlock();
    cursor.insertList(QTextListFormat::ListDecimal);
    cursor.insertText(QStringLiteral("list item"));

    QTextImageFormat image;
    image.setName(QStringLiteral("asset:stable-asset-id"));
    image.setProperty(QTextFormat::UserProperty + 12, QStringLiteral("Map"));
    cursor.insertImage(image);

    QTextCharFormat opaque;
    opaque.setObjectType(QTextFormat::UserObject + 2);
    opaque.setProperty(opaqueSourceProperty, QStringLiteral("@future[exact]"));
    cursor.insertText(QString(QChar::ObjectReplacementCharacter), opaque);

    QTextDocument copy;
    QTextCursor copyCursor(&copy);
    copyCursor.insertFragment(QTextDocumentFragment(&document));
    const auto first = copy.begin();
    QCOMPARE(first.blockFormat().headingLevel(), 1);
    QCOMPARE(first.blockFormat().alignment(), Qt::AlignHCenter);
    QCOMPARE(first.blockFormat().property(stableStyleProperty).toString(),
             QStringLiteral("style-heading-uuid"));
    QVERIFY(copy.toPlainText().contains(QStringLiteral("Heading")));
    QVERIFY(copy.toHtml().contains(QStringLiteral("https://example.invalid")));

    bool foundImage = false;
    bool foundOpaque = false;
    for (auto block = copy.begin(); block.isValid(); block = block.next()) {
      for (auto fragment = block.begin(); !fragment.atEnd(); ++fragment) {
        const auto format = fragment.fragment().charFormat();
        foundImage |= format.isImageFormat()
          && format.toImageFormat().name() == QStringLiteral("asset:stable-asset-id");
        foundOpaque |= format.objectType() == QTextFormat::UserObject + 2
          && format.property(opaqueSourceProperty).toString() == QStringLiteral("@future[exact]");
      }
    }
    QVERIFY(foundImage);
    QVERIFY(foundOpaque);
  }

  void plainAndRichPasteTakeDistinctPaths()
  {
    QTextDocument document;
    QTextCursor cursor(&document);
    cursor.insertFragment(QTextDocumentFragment::fromPlainText(QStringLiteral("**literal**")));
    QCOMPARE(document.toPlainText(), QStringLiteral("**literal**"));
    QVERIFY(document.begin().begin().fragment().charFormat().fontWeight() < QFont::Bold);

    cursor.movePosition(QTextCursor::End);
    cursor.insertBlock();
    cursor.insertFragment(QTextDocumentFragment::fromHtml(
      QStringLiteral("<strong>bold</strong><script>active()</script>")));
    QVERIFY(document.toPlainText().contains(QStringLiteral("bold")));
    QVERIFY(!document.toPlainText().contains(QStringLiteral("active")));
  }

  void protectedInsertionResetsToNextStyleAndRequiresExplicitDeletion()
  {
    auto checkNextStyle = [](auto insert) {
      QTextDocument document;
      document.setPlainText(QStringLiteral("Heading"));
      EditorAdapter adapter;
      adapter.setDocumentForTesting(&document);
      adapter.defineStyle(QStringLiteral("body"), {}, true, QStringLiteral("body"));
      adapter.defineStyle(QStringLiteral("heading"), {}, true, QStringLiteral("body"));
      adapter.setCursorPosition(7);
      adapter.setSelectionStart(7);
      adapter.setSelectionEnd(7);
      adapter.setParagraphStyle(QStringLiteral("heading"));
      insert(adapter);

      const auto last = document.lastBlock();
      QCOMPARE(last.blockFormat().property(QTextFormat::UserProperty + 10).toString(),
               QStringLiteral("body"));
      const auto format = last.begin().atEnd() ? QTextCharFormat()
                                                : last.begin().fragment().charFormat();
      QVERIFY(!format.fontUnderline());
      QVERIFY(format.objectType() < QTextFormat::UserObject);
    };
    checkNextStyle([](EditorAdapter& adapter) { adapter.insertPageBreak(); });
    checkNextStyle([](EditorAdapter& adapter) {
      adapter.insertOpaqueBlock(QStringLiteral("@future[value]"), QStringLiteral("future"));
    });
    checkNextStyle([](EditorAdapter& adapter) { adapter.insertSceneBreak(); });

    QTextDocument document;
    EditorAdapter adapter;
    adapter.setDocumentForTesting(&document);
    adapter.insertPageBreak();
    adapter.setSelectionStart(0);
    adapter.setSelectionEnd(1);
    QSignalSpy errors(&adapter, &EditorAdapter::adapterError);
    adapter.pastePlainText(QStringLiteral("cannot replace protected"));
    QCOMPARE(errors.size(), 1);
    QVERIFY(document.toPlainText().contains(QChar::ObjectReplacementCharacter));
    adapter.deletePreviousSemanticUnit();
    QVERIFY(!document.toPlainText().contains(QChar::ObjectReplacementCharacter));
  }

  void richPasteProjectsOnlyAllowedSemanticRuns()
  {
    QTextDocument document;
    EditorAdapter adapter;
    adapter.setDocumentForTesting(&document);
    adapter.pasteRichHtml(QStringLiteral(
      "<img src='https://remote.invalid/tracker.png'>"
      "<a href='ftp://remote.invalid'>unsafe</a>"
      "<a href='https://example.invalid'>safe</a>"
      "<u>underlined</u><script>active()</script>"));

    QVERIFY(document.toPlainText().contains(QStringLiteral("unsafe")));
    QVERIFY(document.toPlainText().contains(QStringLiteral("safe")));
    QVERIFY(document.toPlainText().contains(QStringLiteral("underlined")));
    QVERIFY(!document.toPlainText().contains(QStringLiteral("active")));

    bool hasImage = false;
    bool safeLink = false;
    bool unsafeLink = false;
    bool underlined = false;
    for (auto block = document.begin(); block.isValid(); block = block.next()) {
      for (auto it = block.begin(); !it.atEnd(); ++it) {
        const auto fragment = it.fragment();
        const auto format = fragment.charFormat();
        hasImage |= format.isImageFormat();
        safeLink |= format.isAnchor() && format.anchorHref() == QStringLiteral("https://example.invalid");
        unsafeLink |= format.isAnchor() && format.anchorHref().startsWith(QStringLiteral("ftp:"));
        underlined |= fragment.text().contains(QStringLiteral("underlined")) && format.fontUnderline();
      }
    }
    QVERIFY(!hasImage);
    QVERIFY(safeLink);
    QVERIFY(!unsafeLink);
    QVERIFY(underlined);
  }

  void groupedListsAndUnderlineSurviveSemanticProjection()
  {
    QTextDocument document;
    document.setPlainText(QStringLiteral("first\nsecond"));
    EditorAdapter adapter;
    adapter.setDocumentForTesting(&document);
    adapter.setSelectionStart(0);
    adapter.setSelectionEnd(document.characterCount() - 1);
    adapter.toggleList(true);
    auto first = document.begin();
    auto second = first.next();
    QVERIFY(first.textList());
    QCOMPARE(first.textList(), second.textList());

    adapter.setSelectionStart(0);
    adapter.setSelectionEnd(5);
    adapter.toggleUnderline();
    const auto blocks = adapter.semanticBlocks();
    QVERIFY(blocks[0].toMap().value(QStringLiteral("runs")).toList()[0]
              .toMap()
              .value(QStringLiteral("underline"))
              .toBool());
  }

  void unicodeFixtureHasStableGraphemeBoundaries()
  {
    const QString text = QStringLiteral("café 👩🏽‍💻 क्ष שלום مرحبا");
    QTextBoundaryFinder finder(QTextBoundaryFinder::Grapheme, text);
    int boundaries = 0;
    while (finder.toNextBoundary() != -1)
      ++boundaries;
    QVERIFY(boundaries > 10);
    QVERIFY(boundaries < text.size());
  }

  void nonLatinImeCommitAndDeadKeyCompositionReachQuickEditor()
  {
    QQmlEngine engine;
    QQmlComponent component(&engine);
    component.setData(R"(
      import QtQuick
      import QtQuick.Controls
      TextArea { text: ""; focus: true }
    )", QUrl(QStringLiteral("inmemory:/ImeHarness.qml")));
    QTRY_VERIFY_WITH_TIMEOUT(component.status() != QQmlComponent::Loading, 5'000);
    QVERIFY2(component.status() == QQmlComponent::Ready, qPrintable(component.errorString()));
    QScopedPointer<QObject> editor(component.create());
    QVERIFY2(editor, qPrintable(component.errorString()));

    QInputMethodEvent preedit(QStringLiteral("に"), {});
    QVERIFY(QCoreApplication::sendEvent(editor.data(), &preedit));
    QInputMethodEvent japanese;
    japanese.setCommitString(QStringLiteral("日本語"));
    QVERIFY(QCoreApplication::sendEvent(editor.data(), &japanese));
    QInputMethodEvent deadKey(QStringLiteral("´"), {});
    QVERIFY(QCoreApplication::sendEvent(editor.data(), &deadKey));
    QInputMethodEvent composed;
    composed.setCommitString(QStringLiteral("é"));
    QVERIFY(QCoreApplication::sendEvent(editor.data(), &composed));

    QCOMPARE(editor->property("text").toString(), QStringLiteral("日本語é"));
  }
};

QTEST_MAIN(EditorAdapterTest)
#include "tst_editor_adapter.moc"
#include <QCoreApplication>
#include <QInputMethodEvent>
#include <QQmlComponent>
#include <QQmlEngine>
#include <QScopedPointer>
