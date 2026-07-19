#include "editor_adapter.h"

#include <QCoreApplication>
#include <QInputMethodEvent>
#include <QQmlComponent>
#include <QQmlEngine>
#include <QScopedPointer>
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
