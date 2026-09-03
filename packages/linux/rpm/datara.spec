Name:           datara
Version:        0.1.0
Release:        1%{?dist}
Summary:        High-performance Datara programming language and Forgen compiler
License:        MIT or Apache-2.0
URL:            https://github.com/waters1ze/datara
BuildArch:      x86_64

%description
Datara is a next-generation systems programming language featuring
evidence-gated SSA optimizations, zero GC pauses, and a multi-tier
compilation ladder (JIT, Cranelift AOT, LLVM Peak AOT).

%install
mkdir -p %{buildroot}/usr/bin
mkdir -p %{buildroot}/usr/lib/datara
cp -r %{_sourcedir}/bin/* %{buildroot}/usr/bin/
cp -r %{_sourcedir}/stdlib %{buildroot}/usr/lib/datara/
cp -r %{_sourcedir}/runtime %{buildroot}/usr/lib/datara/

%files
/usr/bin/forgen
/usr/bin/datara
/usr/bin/dpm
/usr/lib/datara

%changelog
* Wed Sep 03 2026 waters1ze <https://github.com/waters1ze/datara> - 0.1.0-1
- Initial public release of Datara and Forgen compiler toolchain