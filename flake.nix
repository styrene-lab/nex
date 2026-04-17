{
  description = "Package manager UX for nix-darwin + homebrew";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
  };

  outputs = { self, nixpkgs }:
    let
      supportedSystems = [ "aarch64-darwin" "x86_64-darwin" "aarch64-linux" "x86_64-linux" ];
      forAllSystems = nixpkgs.lib.genAttrs supportedSystems;
    in
    {
      packages = forAllSystems (system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
        in
        {
          default = pkgs.rustPlatform.buildRustPackage {
            pname = "nex";
            version = "0.6.1";
            src = ./.;
            cargoLock.lockFile = ./Cargo.lock;
            meta = with pkgs.lib; {
              description = "Package manager UX for nix-darwin + homebrew";
              homepage = "https://nex.styrene.io";
              license = with licenses; [ mit asl20 ];
              mainProgram = "nex";
            };
          };
        }
      );
    };
}
