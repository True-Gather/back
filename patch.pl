undef $/;
open(my $f, "<", "src/auth/session.rs") or die $!;
my $content = <$f>;
close($f);

$content =~ s/if let Some\(\(name, value\)\) = trimmed\.split_once\('=+\'\) \{\s*if name == cookie_name \{\s*return Some\(value\.to_string\(\)\);\s*\}\s*\}/if let Some((name, value)) = trimmed.split_once('=') {\n            if name == cookie_name {\n                return Some(value.to_string());\n            }\n        }/;

open($f, ">", "src/auth/session.rs") or die $!;
print $f $content;
close($f);
