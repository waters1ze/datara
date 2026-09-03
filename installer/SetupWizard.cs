using System;
using System.Drawing;
using System.IO;
using System.IO.Compression;
using System.Reflection;
using System.Runtime.InteropServices;
using System.Threading.Tasks;
using System.Windows.Forms;
using Microsoft.Win32;

namespace DataraInstaller
{
    static class Program
    {
        [STAThread]
        static void Main()
        {
            Application.EnableVisualStyles();
            Application.SetCompatibleTextRenderingDefault(false);
            Application.Run(new InstallerForm());
        }
    }

    public class InstallerForm : Form
    {
        private Panel headerPanel;
        private Label titleLabel;
        private Label subtitleLabel;
        private PictureBox logoBox;

        private Panel configPanel;
        private Label installDirLabel;
        private TextBox txtInstallDir;
        private Button btnBrowse;

        private CheckBox chkPath;
        private CheckBox chkAssoc;
        private CheckBox chkVSCode;
        private CheckBox chkStdlib;

        private Panel progressPanel;
        private Label statusLabel;
        private ProgressBar progressBar;

        private Panel successPanel;
        private Label successTitle;
        private Label successSubtitle;
        private RichTextBox tipBox;

        private Panel footerPanel;
        private Label footerBrand;
        private Button btnInstall;
        private Button btnCancel;
        private Button btnClose;

        [DllImport("Shell32.dll")]
        public static extern void SHChangeNotify(int eventId, int flags, IntPtr item1, IntPtr item2);

        public InstallerForm()
        {
            InitializeComponent();
        }

        private void InitializeComponent()
        {
            this.Text = "Datara Language & Forgen Compiler Setup v0.1.0";
            this.Size = new Size(680, 520);
            this.StartPosition = FormStartPosition.CenterScreen;
            this.FormBorderStyle = FormBorderStyle.FixedDialog;
            this.MaximizeBox = false;
            this.BackColor = Color.FromArgb(15, 23, 42); // #0F172A
            this.Font = new Font("Segoe UI", 9.5f);

            // Try load icon from resource or exe
            try {
                this.Icon = Icon.ExtractAssociatedIcon(Application.ExecutablePath);
            } catch { }

            // 1. Header Panel
            headerPanel = new Panel {
                Dock = DockStyle.Top,
                Height = 84,
                BackColor = Color.FromArgb(30, 41, 59) // #1E293B
            };

            logoBox = new PictureBox {
                Location = new Point(24, 16),
                Size = new Size(52, 52),
                SizeMode = PictureBoxSizeMode.Zoom
            };
            if (this.Icon != null) {
                logoBox.Image = this.Icon.ToBitmap();
            }

            titleLabel = new Label {
                Text = "Datara Setup Wizard",
                Location = new Point(88, 16),
                AutoSize = true,
                Font = new Font("Segoe UI", 15f, FontStyle.Bold),
                ForeColor = Color.FromArgb(56, 189, 248) // #38BDF8
            };

            subtitleLabel = new Label {
                Text = "High-Performance Post-OOP Systems & Application Language",
                Location = new Point(90, 46),
                AutoSize = true,
                Font = new Font("Segoe UI", 9.5f),
                ForeColor = Color.FromArgb(148, 163, 184) // #94A3B8
            };

            headerPanel.Controls.Add(logoBox);
            headerPanel.Controls.Add(titleLabel);
            headerPanel.Controls.Add(subtitleLabel);

            // 2. Footer Panel
            footerPanel = new Panel {
                Dock = DockStyle.Bottom,
                Height = 64,
                BackColor = Color.FromArgb(11, 17, 32) // #0B1120
            };

            footerBrand = new Label {
                Text = "Datara Project • Apache 2.0 / MIT Open Source",
                Location = new Point(24, 22),
                AutoSize = true,
                Font = new Font("Segoe UI", 9f),
                ForeColor = Color.FromArgb(100, 116, 139)
            };

            btnCancel = new Button {
                Text = "Cancel",
                Location = new Point(440, 14),
                Size = new Size(96, 36),
                FlatStyle = FlatStyle.Flat,
                ForeColor = Color.FromArgb(203, 213, 225),
                BackColor = Color.FromArgb(30, 41, 59),
                Cursor = Cursors.Hand
            };
            btnCancel.FlatAppearance.BorderColor = Color.FromArgb(71, 85, 105);
            btnCancel.Click += (s, e) => this.Close();

            btnInstall = new Button {
                Text = "Install Now",
                Location = new Point(546, 14),
                Size = new Size(110, 36),
                FlatStyle = FlatStyle.Flat,
                Font = new Font("Segoe UI", 10f, FontStyle.Bold),
                ForeColor = Color.White,
                BackColor = Color.FromArgb(2, 132, 199), // #0284C7
                Cursor = Cursors.Hand
            };
            btnInstall.FlatAppearance.BorderSize = 0;
            btnInstall.Click += async (s, e) => await StartInstallation();

            btnClose = new Button {
                Text = "Close",
                Location = new Point(546, 14),
                Size = new Size(110, 36),
                FlatStyle = FlatStyle.Flat,
                Font = new Font("Segoe UI", 10f, FontStyle.Bold),
                ForeColor = Color.White,
                BackColor = Color.FromArgb(16, 185, 129), // Green
                Visible = false,
                Cursor = Cursors.Hand
            };
            btnClose.FlatAppearance.BorderSize = 0;
            btnClose.Click += (s, e) => this.Close();

            footerPanel.Controls.Add(footerBrand);
            footerPanel.Controls.Add(btnCancel);
            footerPanel.Controls.Add(btnInstall);
            footerPanel.Controls.Add(btnClose);

            // 3. Config Panel
            configPanel = new Panel {
                Location = new Point(24, 98),
                Size = new Size(632, 310),
                BackColor = Color.Transparent
            };

            Label sectionTitle = new Label {
                Text = "Install Datara 0.1.0 (64-bit)",
                Location = new Point(0, 4),
                AutoSize = true,
                Font = new Font("Segoe UI", 13f, FontStyle.Bold),
                ForeColor = Color.FromArgb(241, 245, 249)
            };

            Label sectionDesc = new Label {
                Text = "Select installation directory and system options below.",
                Location = new Point(2, 30),
                AutoSize = true,
                ForeColor = Color.FromArgb(148, 163, 184)
            };

            installDirLabel = new Label {
                Text = "Installation Folder:",
                Location = new Point(2, 60),
                AutoSize = true,
                Font = new Font("Segoe UI", 9.5f, FontStyle.Bold),
                ForeColor = Color.FromArgb(203, 213, 225)
            };

            string defaultDir = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), @"Programs\Datara");
            txtInstallDir = new TextBox {
                Text = defaultDir,
                Location = new Point(4, 84),
                Size = new Size(516, 28),
                BackColor = Color.FromArgb(30, 41, 59),
                ForeColor = Color.FromArgb(248, 250, 252),
                BorderStyle = BorderStyle.FixedSingle
            };

            btnBrowse = new Button {
                Text = "Browse...",
                Location = new Point(528, 82),
                Size = new Size(98, 28),
                FlatStyle = FlatStyle.Flat,
                ForeColor = Color.White,
                BackColor = Color.FromArgb(51, 65, 85),
                Cursor = Cursors.Hand
            };
            btnBrowse.FlatAppearance.BorderColor = Color.FromArgb(71, 85, 105);
            btnBrowse.Click += (s, e) => {
                using (FolderBrowserDialog fbd = new FolderBrowserDialog()) {
                    fbd.SelectedPath = txtInstallDir.Text;
                    if (fbd.ShowDialog() == DialogResult.OK) {
                        txtInstallDir.Text = fbd.SelectedPath;
                    }
                }
            };

            Panel optBox = new Panel {
                Location = new Point(4, 126),
                Size = new Size(622, 160),
                BackColor = Color.FromArgb(30, 41, 59)
            };

            chkPath = new CheckBox {
                Text = "Add Datara (forgen & datara) to PATH environment variable (Recommended)",
                Location = new Point(16, 16),
                AutoSize = true,
                Checked = true,
                ForeColor = Color.FromArgb(241, 245, 249)
            };
            chkAssoc = new CheckBox {
                Text = "Associate .dtr files with Datara and show official icon in Windows Explorer",
                Location = new Point(16, 48),
                AutoSize = true,
                Checked = true,
                ForeColor = Color.FromArgb(241, 245, 249)
            };
            chkVSCode = new CheckBox {
                Text = "Install Datara Syntax Extension for VS Code / Cursor / VSCodium",
                Location = new Point(16, 80),
                AutoSize = true,
                Checked = true,
                ForeColor = Color.FromArgb(241, 245, 249)
            };
            chkStdlib = new CheckBox {
                Text = "Install complete Standard Library (14 modules: math, text, io, net, etc.)",
                Location = new Point(16, 112),
                AutoSize = true,
                Checked = true,
                ForeColor = Color.FromArgb(241, 245, 249)
            };

            optBox.Controls.Add(chkPath);
            optBox.Controls.Add(chkAssoc);
            optBox.Controls.Add(chkVSCode);
            optBox.Controls.Add(chkStdlib);

            configPanel.Controls.Add(sectionTitle);
            configPanel.Controls.Add(sectionDesc);
            configPanel.Controls.Add(installDirLabel);
            configPanel.Controls.Add(txtInstallDir);
            configPanel.Controls.Add(btnBrowse);
            configPanel.Controls.Add(optBox);

            // 4. Progress Panel
            progressPanel = new Panel {
                Location = new Point(24, 140),
                Size = new Size(632, 220),
                BackColor = Color.Transparent,
                Visible = false
            };

            Label prgTitle = new Label {
                Text = "Installing Datara...",
                Location = new Point(10, 20),
                AutoSize = true,
                Font = new Font("Segoe UI", 14f, FontStyle.Bold),
                ForeColor = Color.FromArgb(56, 189, 248)
            };

            statusLabel = new Label {
                Text = "Extracting compiler toolchain and standard library...",
                Location = new Point(12, 60),
                AutoSize = true,
                Font = new Font("Segoe UI", 10f),
                ForeColor = Color.FromArgb(203, 213, 225)
            };

            progressBar = new ProgressBar {
                Location = new Point(14, 90),
                Size = new Size(600, 20),
                Minimum = 0,
                Maximum = 100,
                Value = 10
            };

            progressPanel.Controls.Add(prgTitle);
            progressPanel.Controls.Add(statusLabel);
            progressPanel.Controls.Add(progressBar);

            // 5. Success Panel
            successPanel = new Panel {
                Location = new Point(24, 110),
                Size = new Size(632, 280),
                BackColor = Color.Transparent,
                Visible = false
            };

            successTitle = new Label {
                Text = "✓ Setup was successful",
                Location = new Point(6, 10),
                AutoSize = true,
                Font = new Font("Segoe UI", 16f, FontStyle.Bold),
                ForeColor = Color.FromArgb(74, 222, 128) // Green
            };

            successSubtitle = new Label {
                Text = "Datara 0.1.0 is now ready to use on your system!",
                Location = new Point(10, 46),
                AutoSize = true,
                Font = new Font("Segoe UI", 10f),
                ForeColor = Color.FromArgb(226, 232, 240)
            };

            tipBox = new RichTextBox {
                Location = new Point(10, 80),
                Size = new Size(610, 140),
                BackColor = Color.FromArgb(30, 41, 59),
                ForeColor = Color.FromArgb(248, 250, 252),
                BorderStyle = BorderStyle.FixedSingle,
                ReadOnly = true,
                Font = new Font("Consolas", 10f)
            };
            tipBox.AppendText("Quick Start Commands:\n\n");
            tipBox.AppendText("  forgen --version     # Check compiler version and native target\n");
            tipBox.AppendText("  dpm init my_app      # Initialize a project with Datara Package Manager\n");
            tipBox.AppendText("  dpm add redis        # Add packages with SHA-256 Merkle verification\n");
            tipBox.AppendText("  forgen run main.dtr  # Compile & run Datara source code\n\n");
            tipBox.AppendText("All .dtr files are now associated with the official Datara icon.");

            successPanel.Controls.Add(successTitle);
            successPanel.Controls.Add(successSubtitle);
            successPanel.Controls.Add(tipBox);

            // Add all panels to form
            this.Controls.Add(configPanel);
            this.Controls.Add(progressPanel);
            this.Controls.Add(successPanel);
            this.Controls.Add(headerPanel);
            this.Controls.Add(footerPanel);
        }

        private async Task StartInstallation()
        {
            string installDir = txtInstallDir.Text.Trim();
            bool doPath = chkPath.Checked;
            bool doAssoc = chkAssoc.Checked;
            bool doVSCode = chkVSCode.Checked;
            bool doStdlib = chkStdlib.Checked;

            configPanel.Visible = false;
            progressPanel.Visible = true;
            btnInstall.Visible = false;
            btnCancel.Enabled = false;

            await Task.Run(() => {
                try {
                    UpdateProgress(20, "Extracting compiler toolchain and assets...");
                    Directory.CreateDirectory(installDir);

                    // Extract embedded payload.zip
                    Assembly asm = Assembly.GetExecutingAssembly();
                    using (Stream resStream = asm.GetManifestResourceStream("payload.zip")) {
                        if (resStream != null) {
                            using (ZipArchive archive = new ZipArchive(resStream, ZipArchiveMode.Read)) {
                                foreach (ZipArchiveEntry entry in archive.Entries) {
                                    string fullPath = Path.Combine(installDir, entry.FullName);
                                    if (string.IsNullOrEmpty(entry.Name)) {
                                        Directory.CreateDirectory(fullPath);
                                    } else {
                                        string dir = Path.GetDirectoryName(fullPath);
                                        if (!string.IsNullOrEmpty(dir)) Directory.CreateDirectory(dir);
                                        entry.ExtractToFile(fullPath, true);
                                    }
                                }
                            }
                        }
                    }

                    // Step 2: Register PATH
                    if (doPath) {
                        UpdateProgress(50, "Configuring PATH and environment variables...");
                        string binDir = Path.Combine(installDir, "bin");
                        string userPath = Environment.GetEnvironmentVariable("PATH", EnvironmentVariableTarget.User) ?? "";
                        if (!userPath.Contains(binDir)) {
                            string newPath = binDir + ";" + userPath;
                            Environment.SetEnvironmentVariable("PATH", newPath, EnvironmentVariableTarget.User);
                        }
                        Environment.SetEnvironmentVariable("DATARA_HOME", installDir, EnvironmentVariableTarget.User);
                    }

                    // Step 3: Register File Associations (.dtr)
                    if (doAssoc) {
                        UpdateProgress(70, "Associating .dtr files with official Datara icon...");
                        RegisterFileAssociation(installDir);
                    }

                    // Step 4: Register in Windows Installed Apps
                    UpdateProgress(85, "Creating Windows Uninstall registration...");
                    RegisterUninstall(installDir);

                    // Step 5: Install VS Code Extension
                    if (doVSCode) {
                        UpdateProgress(95, "Installing VS Code syntax highlighting...");
                        InstallVSCodeExtension(installDir);
                    }

                    UpdateProgress(100, "Installation complete!");
                } catch (Exception ex) {
                    MessageBox.Show("Installation Error: " + ex.Message, "Error", MessageBoxButtons.OK, MessageBoxIcon.Error);
                }
            });

            progressPanel.Visible = false;
            successPanel.Visible = true;
            btnCancel.Visible = false;
            btnClose.Visible = true;
        }

        private void UpdateProgress(int percent, string status)
        {
            if (this.InvokeRequired) {
                this.Invoke(new Action(() => UpdateProgress(percent, status)));
                return;
            }
            progressBar.Value = percent;
            statusLabel.Text = status;
        }

        private void RegisterFileAssociation(string installDir)
        {
            try {
                string icoPath = Path.Combine(installDir, @"assets\datara.ico");
                string forgenPath = Path.Combine(installDir, @"bin\forgen.exe");

                // HKCU\Software\Classes\.dtr
                using (RegistryKey dtrKey = Registry.CurrentUser.CreateSubKey(@"Software\Classes\.dtr")) {
                    dtrKey.SetValue("", "DataraSourceFile");
                    dtrKey.SetValue("Content Type", "text/plain");
                    dtrKey.SetValue("PerceivedType", "text");
                    using (RegistryKey dtrIconKey = dtrKey.CreateSubKey("DefaultIcon")) {
                        dtrIconKey.SetValue("", icoPath);
                    }
                }

                // HKCU\Software\Classes\DataraSourceFile
                using (RegistryKey progKey = Registry.CurrentUser.CreateSubKey(@"Software\Classes\DataraSourceFile")) {
                    progKey.SetValue("", "Datara Source File");
                    progKey.SetValue("FriendlyTypeName", "Datara Source File (.dtr)");

                    using (RegistryKey iconKey = progKey.CreateSubKey("DefaultIcon")) {
                        iconKey.SetValue("", icoPath);
                    }

                    using (RegistryKey openKey = progKey.CreateSubKey(@"shell\open\command")) {
                        openKey.SetValue("", "\"" + forgenPath + "\" run \"%1\"");
                    }

                    using (RegistryKey editKey = progKey.CreateSubKey(@"shell\edit\command")) {
                        editKey.SetValue("", "notepad.exe \"%1\"");
                    }
                }

                // Also register Datara.SourceFile alias with icon
                using (RegistryKey dotProgKey = Registry.CurrentUser.CreateSubKey(@"Software\Classes\Datara.SourceFile")) {
                    dotProgKey.SetValue("", "Datara Source File");
                    using (RegistryKey iconKey2 = dotProgKey.CreateSubKey("DefaultIcon")) {
                        iconKey2.SetValue("", icoPath);
                    }
                }

                // Notify Windows Explorer immediately
                SHChangeNotify(0x08000000, 0, IntPtr.Zero, IntPtr.Zero);
            } catch { }
        }

        private void RegisterUninstall(string installDir)
        {
            try {
                string uninstKey = @"Software\Microsoft\Windows\CurrentVersion\Uninstall\Datara";
                using (RegistryKey key = Registry.CurrentUser.CreateSubKey(uninstKey)) {
                    key.SetValue("DisplayName", "Datara Language & Forgen Compiler");
                    key.SetValue("DisplayVersion", "0.1.0");
                    key.SetValue("Publisher", "Datara Language Project");
                    key.SetValue("DisplayIcon", Path.Combine(installDir, @"assets\datara.ico"));
                    key.SetValue("InstallLocation", installDir);
                    key.SetValue("UninstallString", "\"" + Application.ExecutablePath + "\" /uninstall");
                    key.SetValue("URLInfoAbout", "https://github.com/waters1ze/datara");
                }
            } catch { }
        }

        private void InstallVSCodeExtension(string installDir)
        {
            try {
                string vscodeExtSrc = Path.Combine(installDir, @"editors\vscode");
                string userProfile = Environment.GetFolderPath(Environment.SpecialFolder.UserProfile);
                string vscodeExtDst = Path.Combine(userProfile, @".vscode\extensions\datara-language");

                if (Directory.Exists(vscodeExtSrc)) {
                    Directory.CreateDirectory(vscodeExtDst);
                    foreach (string dir in Directory.GetDirectories(vscodeExtSrc, "*", SearchOption.AllDirectories)) {
                        Directory.CreateDirectory(dir.Replace(vscodeExtSrc, vscodeExtDst));
                    }
                    foreach (string file in Directory.GetFiles(vscodeExtSrc, "*.*", SearchOption.AllDirectories)) {
                        File.Copy(file, file.Replace(vscodeExtSrc, vscodeExtDst), true);
                    }
                }
            } catch { }
        }
    }
}
