FROM mcr.microsoft.com/dotnet/sdk:5.0 as build-env
WORKDIR /ribble-server

COPY *.csproj ./

RUN apt-get update
RUN apt-get upgrade -y

RUN dotnet restore
RUN dotnet tool install --global dotnet-ef
ENV PATH="/root/.dotnet/tools:${PATH}"

COPY * ./

# RUN dotnet publish -c Release -o out
# RUN dotnet publish -o out

# FROM mcr.microsoft.com/dotnet/aspnet:5.0
# WORKDIR /ribble-server
# COPY --from=build-env /ribble-server/out  .

EXPOSE 5000
EXPOSE 5001

# ENTRYPOINT ["dotnet", "RibbleChatServer.dll"]
# ENTRYPOINT ["dotnet", "/ribble-server/bin/Release/net5.0/RibbleChatServer.dll"]
ENTRYPOINT ["dotnet", "watch", "run"]
