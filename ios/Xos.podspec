Pod::Spec.new do |s|
  s.name           = 'Xos'
  s.version        = '0.1.23'
  s.summary        = 'Experimental OS Window Manager'
  s.description    = 'Experimental OS Window Manager with Rust backend'
  s.license        = 'MIT'
  s.author         = { 'Dyllan McCreary' => 'dyllan@xlate.ai' }
  s.homepage       = 'https://github.com/xlateai/xos'
  s.platforms      = {
    :ios => '15.1',
    :tvos => '15.1'
  }
  s.swift_version  = '5.9'
  s.source         = { git: 'https://github.com/xlateai/xos' }
  s.static_framework = true

  # Rust library - vendored_libraries automatically links it
  s.vendored_libraries = "libs/libxos.a"
  # Only include files from XosModule directory (not the main app)
  s.source_files = "XosModule/**/*.{h,m,mm,swift,hpp,cpp}"
  s.public_header_files = "XosModule/**/*.h"
  
  # Required frameworks for CoreAudio support (used by Rust library)
  s.frameworks = 'AudioToolbox', 'AVFoundation'
  
  # Swift/Objective-C compatibility
  s.pod_target_xcconfig = {
    'DEFINES_MODULE' => 'YES',
  }
end

