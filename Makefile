

XCODE_PROJECT := screenhistory.xcodeproj
XCODE_SCHEME := screenhistory
CONFIGURATION := Debug

build:
	xcrun xcodebuild \
	  -scheme $(XCODE_SCHEME) \
	  -project $(XCODE_PROJECT) \
	  -configuration $(CONFIGURATION) \
	  -destination 'platform=macOS' \
	  -derivedDataPath \
	  build

run: build
	open build/Build/Products/$(CONFIGURATION)/screenhistory.app
