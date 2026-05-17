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

  # Rust static library is built by `xos compile --ios` → src/crates/xos-ios/libs/libxos.a.
  # The main **app** target force-loads it (see Podfile): static frameworks do not reliably
  # propagate a vendored .a through to the final app link.
  s.preserve_paths = 'libs/libxos.a'
  # Only include files from XosModule directory (not the main app)
  s.source_files = "XosModule/**/*.{h,m,mm,swift,hpp,cpp}"
  s.public_header_files = "XosModule/**/*.h"
  
  # Required frameworks for CoreAudio and CoreMotion support (used by Rust library)
  s.frameworks = 'AudioToolbox', 'AVFoundation', 'CoreMotion'
  
  s.pod_target_xcconfig = {
    'DEFINES_MODULE' => 'YES',
  }
end

