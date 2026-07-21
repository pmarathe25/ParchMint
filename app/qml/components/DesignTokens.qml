pragma Singleton
import QtQuick

QtObject {
    // Neutral chrome. Accent is reserved for direct intent: selection, focus,
    // links, and the primary action.
    readonly property color base: "#f7f7f5"
    readonly property color surface: "#fcfcfa"
    readonly property color raised: "#ffffff"
    readonly property color overlay: "#f0f1ee"
    readonly property color darkBase: "#171817"
    readonly property color darkSurface: "#1e201f"
    readonly property color darkRaised: "#282a28"
    readonly property color text: "#202421"
    readonly property color textMuted: "#626b65"
    readonly property color textFaint: "#8a918c"
    readonly property color outline: "#d7dbd6"
    readonly property color accent: "#147b70"
    readonly property color accentContainer: "#d8eee9"
    readonly property color danger: "#b42318"
    readonly property color dangerContainer: "#fee4e2"

    readonly property int typeCaption: 12
    readonly property int typeBody: 15
    readonly property int typeTitle: 18
    readonly property int typeDisplay: 24
    readonly property real editorLineHeight: 1.56
    readonly property int iconSize: 18
    readonly property int iconTouch: 36
    readonly property int space1: 4
    readonly property int space2: 8
    readonly property int space3: 12
    readonly property int space4: 16
    readonly property int space5: 20
    readonly property int space6: 24
    readonly property int space8: 32
    readonly property int radiusSmall: 5
    readonly property int radiusMedium: 8
    readonly property int radiusLarge: 12
    readonly property int elevationLow: 1
    readonly property int elevationRaised: 3
    readonly property int motionFast: 100
    readonly property int motionNormal: 160
}
